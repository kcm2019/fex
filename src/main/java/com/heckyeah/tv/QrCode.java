package com.heckyeah.tv;

/**
 * Minimal QR Code generator (byte mode, error correction level M,
 * versions 1..10) producing the module matrix for rendering.
 *
 * The algorithm follows the structure of the QR Code generator
 * library by Nayuki (https://www.nayuki.io/page/qr-code-generator-library),
 * which is MIT-licensed; this is a compact re-implementation reduced
 * to the single segment type and ECC level this app needs. Reed-Solomon
 * error correction per ISO/IEC 18004. No dependencies.
 */
public final class QrCode {

    // ECC level M tables, indexed by version - 1 (versions 1..10).
    private static final int[] ECC_CODEWORDS_PER_BLOCK_M =
            {10, 16, 26, 18, 24, 16, 18, 22, 22, 26};
    private static final int[] NUM_ERROR_CORRECTION_BLOCKS_M =
            {1, 1, 1, 2, 2, 4, 4, 4, 5, 5};

    private static final int MAX_VERSION = 10;

    private final int version;
    private final int size;
    private final boolean[][] modules;

    private boolean[][] isFunction;

    private QrCode(int version, byte[] dataCodewords) {
        this.version = version;
        this.size = version * 4 + 17;
        this.modules = new boolean[size][size];
        this.isFunction = new boolean[size][size];

        drawFunctionPatterns();
        byte[] allCodewords = addEccAndInterleave(dataCodewords);
        drawCodewords(allCodewords);

        // Automatic mask selection by lowest penalty score.
        int bestMask = 0;
        int minPenalty = Integer.MAX_VALUE;
        for (int m = 0; m < 8; m++) {
            applyMask(m);
            drawFormatBits(m);
            int penalty = getTotalPenaltyScore();
            if (penalty < minPenalty) {
                bestMask = m;
                minPenalty = penalty;
            }
            applyMask(m); // undo (mask application is its own inverse)
        }
        applyMask(bestMask);
        drawFormatBits(bestMask);
        isFunction = null; // no longer needed after construction
    }

    /** Encodes text (UTF-8, byte mode) at ECC level M. */
    public static QrCode encodeText(String text) {
        byte[] data;
        try {
            data = text.getBytes("UTF-8");
        } catch (Exception e) {
            throw new IllegalArgumentException(e);
        }
        for (int ver = 1; ver <= MAX_VERSION; ver++) {
            int countBits = ver <= 9 ? 8 : 16;
            int capacityBits = getNumDataCodewords(ver) * 8;
            int usedBits = 4 + countBits + data.length * 8;
            if (usedBits > capacityBits) continue;

            BitBuffer bb = new BitBuffer();
            bb.appendBits(0x4, 4); // byte mode indicator
            bb.appendBits(data.length, countBits);
            for (byte b : data) bb.appendBits(b & 0xFF, 8);
            // Terminator + byte alignment + pad bytes.
            bb.appendBits(0, Math.min(capacityBits - bb.length(), 4));
            bb.appendBits(0, (8 - bb.length() % 8) % 8);
            byte[] packed = bb.getBytes();
            int dataCw = getNumDataCodewords(ver);
            byte[] padded = new byte[dataCw];
            System.arraycopy(packed, 0, padded, 0, Math.min(packed.length, dataCw));
            for (int i = packed.length; i < dataCw; i++)
                padded[i] = (byte) (((i - packed.length) % 2 == 0) ? 0xEC : 0x11);
            return new QrCode(ver, padded);
        }
        throw new IllegalArgumentException("Text too long for QR version " + MAX_VERSION);
    }

    public int version() { return version; }
    public int size() { return size; }

    /** Module at (x, y); true = dark. Coordinates are 0-based, no quiet zone. */
    public boolean getModule(int x, int y) {
        return 0 <= x && x < size && 0 <= y && y < size && modules[y][x];
    }

    // ---------------------------------------------------------- capacities

    private static int getNumRawDataModules(int ver) {
        int result = (16 * ver + 128) * ver + 64;
        if (ver >= 2) {
            int numAlign = ver / 7 + 2;
            result -= (25 * numAlign - 10) * numAlign - 55;
            if (ver >= 7) result -= 36;
        }
        return result;
    }

    private static int getNumDataCodewords(int ver) {
        return getNumRawDataModules(ver) / 8
                - ECC_CODEWORDS_PER_BLOCK_M[ver - 1]
                * NUM_ERROR_CORRECTION_BLOCKS_M[ver - 1];
    }

    // ---------------------------------------------------------- function patterns

    private void setFunctionModule(int x, int y, boolean dark) {
        if (0 <= x && x < size && 0 <= y && y < size) {
            modules[y][x] = dark;
            isFunction[y][x] = true;
        }
    }

    private void drawFunctionPatterns() {
        // Timing patterns.
        for (int i = 0; i < size; i++) {
            setFunctionModule(6, i, i % 2 == 0);
            setFunctionModule(i, 6, i % 2 == 0);
        }
        // Finder patterns.
        drawFinderPattern(3, 3);
        drawFinderPattern(size - 4, 3);
        drawFinderPattern(3, size - 4);
        // Alignment patterns.
        int[] pos = getAlignmentPatternPositions();
        for (int i = 0; i < pos.length; i++) {
            for (int j = 0; j < pos.length; j++) {
                boolean corner = (i == 0 && j == 0) || (i == 0 && j == pos.length - 1)
                        || (i == pos.length - 1 && j == 0);
                if (!corner) drawAlignmentPattern(pos[j], pos[i]);
            }
        }
        // Reserve format bits (real values drawn after mask choice).
        drawFormatBits(0);
        // Version info (versions 7+).
        if (version >= 7) {
            int rem = version;
            for (int i = 0; i < 12; i++)
                rem = (rem << 1) ^ ((rem >>> 11) * 0x1F25);
            int bits = version << 12 | rem;
            for (int i = 0; i < 18; i++) {
                boolean bit = getBit(bits, i);
                setFunctionModule(size - 11 + i % 3, i / 3, bit);
                setFunctionModule(i / 3, size - 11 + i % 3, bit);
            }
        }
    }

    private void drawFinderPattern(int x, int y) {
        for (int dy = -4; dy <= 4; dy++) {
            for (int dx = -4; dx <= 4; dx++) {
                int dist = Math.max(Math.abs(dx), Math.abs(dy));
                setFunctionModule(x + dx, y + dy, dist != 2 && dist != 4);
            }
        }
    }

    private void drawAlignmentPattern(int x, int y) {
        for (int dy = -2; dy <= 2; dy++)
            for (int dx = -2; dx <= 2; dx++)
                setFunctionModule(x + dx, y + dy,
                        Math.max(Math.abs(dx), Math.abs(dy)) != 1);
    }

    private int[] getAlignmentPatternPositions() {
        if (version == 1) return new int[0];
        int num = version / 7 + 2;
        int step = (version * 4 + num * 2 + 1) / (num * 2 - 2) * 2;
        int[] result = new int[num];
        result[0] = 6;
        for (int i = result.length - 1, p = size - 7; i >= 1; i--, p -= step)
            result[i] = p;
        return result;
    }

    private void drawFormatBits(int mask) {
        int data = getFormatBits(mask);
        int i;
        for (i = 0; i <= 5; i++)
            setFunctionModule(8, i, getBit(data, i));
        setFunctionModule(8, 7, getBit(data, 6));
        setFunctionModule(8, 8, getBit(data, 7));
        setFunctionModule(7, 8, getBit(data, 8));
        for (i = 9; i < 15; i++)
            setFunctionModule(14 - i, 8, getBit(data, i));
        for (i = 0; i < 8; i++)
            setFunctionModule(size - 1 - i, 8, getBit(data, i));
        for (i = 8; i < 15; i++)
            setFunctionModule(8, size - 15 + i, getBit(data, i));
        setFunctionModule(8, size - 8, true); // always-dark module
    }

    /** 15-bit format information for ECC level M (format bits 00). */
    private static int getFormatBits(int mask) {
        int data = mask; // (0b00 << 3) | mask
        int rem = data;
        for (int i = 0; i < 10; i++)
            rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
        return ((data << 10) | rem) ^ 0x5412;
    }

    private static boolean getBit(int x, int i) {
        return ((x >>> i) & 1) != 0;
    }

    // ---------------------------------------------------------- data placement

    private void drawCodewords(byte[] data) {
        int i = 0; // bit index into data
        for (int right = size - 1; right >= 1; right -= 2) {
            if (right == 6) right = 5;
            for (int vert = 0; vert < size; vert++) {
                for (int j = 0; j < 2; j++) {
                    int x = right - j;
                    boolean upward = ((right + 1) & 2) == 0;
                    int y = upward ? size - 1 - vert : vert;
                    if (!isFunction[y][x] && i < data.length * 8) {
                        modules[y][x] = getBit(data[i >>> 3] & 0xFF, 7 - (i & 7));
                        i++;
                    }
                }
            }
        }
    }

    private void applyMask(int mask) {
        for (int y = 0; y < size; y++) {
            for (int x = 0; x < size; x++) {
                boolean invert;
                switch (mask) {
                    case 0: invert = (x + y) % 2 == 0; break;
                    case 1: invert = y % 2 == 0; break;
                    case 2: invert = x % 3 == 0; break;
                    case 3: invert = (x + y) % 3 == 0; break;
                    case 4: invert = (x / 3 + y / 2) % 2 == 0; break;
                    case 5: invert = (x * y) % 2 + (x * y) % 3 == 0; break;
                    case 6: invert = ((x * y) % 2 + (x * y) % 3) % 2 == 0; break;
                    case 7: invert = ((x + y) % 2 + (x * y) % 3) % 2 == 0; break;
                    default: throw new IllegalArgumentException();
                }
                if (!isFunction[y][x] && invert)
                    modules[y][x] = !modules[y][x];
            }
        }
    }

    // ---------------------------------------------------------- error correction

    private byte[] addEccAndInterleave(byte[] data) {
        int numBlocks = NUM_ERROR_CORRECTION_BLOCKS_M[version - 1];
        int blockEccLen = ECC_CODEWORDS_PER_BLOCK_M[version - 1];
        int numRaw = getNumRawDataModules(version) / 8;
        int shortBlockLen = numRaw / numBlocks;
        int numShortBlocks = numBlocks - numRaw % numBlocks;
        int shortBlockDataLen = shortBlockLen - blockEccLen;

        byte[] rsDiv = reedSolomonComputeDivisor(blockEccLen);
        byte[][] blocks = new byte[numBlocks][];
        for (int i = 0, k = 0; i < numBlocks; i++) {
            int datLen = shortBlockDataLen + (i >= numShortBlocks ? 1 : 0);
            byte[] dat = new byte[datLen];
            System.arraycopy(data, k, dat, 0, datLen);
            k += datLen;
            byte[] block = new byte[shortBlockLen + 1];
            System.arraycopy(dat, 0, block, 0, datLen);
            byte[] ecc = reedSolomonComputeRemainder(dat, rsDiv);
            System.arraycopy(ecc, 0, block, block.length - blockEccLen, blockEccLen);
            blocks[i] = block;
        }
        // Interleave data + ECC codewords (skipping the gap byte
        // at index shortBlockDataLen in short blocks).
        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
        for (int i = 0; i < blocks[0].length; i++) {
            for (int j = 0; j < blocks.length; j++) {
                if (i != shortBlockDataLen || j >= numShortBlocks)
                    out.write(blocks[j][i]);
            }
        }
        byte[] result = out.toByteArray();
        assert result.length == numRaw;
        return result;
    }

    private static byte[] reedSolomonComputeDivisor(int degree) {
        byte[] result = new byte[degree];
        result[result.length - 1] = 1;
        int root = 1;
        for (int i = 0; i < degree; i++) {
            for (int j = 0; j < result.length; j++) {
                result[j] = (byte) reedSolomonMultiply(result[j] & 0xFF, root);
                if (j + 1 < result.length)
                    result[j] ^= result[j + 1];
            }
            root = reedSolomonMultiply(root, 0x02);
        }
        return result;
    }

    private static byte[] reedSolomonComputeRemainder(byte[] data, byte[] divisor) {
        byte[] result = new byte[divisor.length];
        for (byte b : data) {
            int factor = (b ^ result[0]) & 0xFF;
            System.arraycopy(result, 1, result, 0, result.length - 1);
            result[result.length - 1] = 0;
            for (int i = 0; i < result.length; i++)
                result[i] ^= (byte) reedSolomonMultiply(divisor[i] & 0xFF, factor);
        }
        return result;
    }

    private static int reedSolomonMultiply(int x, int y) {
        int z = 0;
        for (int i = 7; i >= 0; i--) {
            z = (z << 1) ^ ((z >>> 7) * 0x11D);
            if (((y >>> i) & 1) != 0) z ^= x;
        }
        return z;
    }

    // ---------------------------------------------------------- penalty scoring (mask choice only)

    private int getTotalPenaltyScore() {
        int score = 0;
        int size = this.size;
        // Adjacent modules in row/column with same color, runs >= 5.
        for (int y = 0; y < size; y++) {
            boolean col = modules[y][0];
            int run = 1;
            for (int x = 1; x < size; x++) {
                if (modules[y][x] == col) { run++; }
                else { if (run >= 5) score += run - 2; col = modules[y][x]; run = 1; }
            }
            if (run >= 5) score += run - 2;
        }
        for (int x = 0; x < size; x++) {
            boolean col = modules[0][x];
            int run = 1;
            for (int y = 1; y < size; y++) {
                if (modules[y][x] == col) { run++; }
                else { if (run >= 5) score += run - 2; col = modules[y][x]; run = 1; }
            }
            if (run >= 5) score += run - 2;
        }
        // 2x2 blocks of the same color.
        for (int y = 0; y < size - 1; y++)
            for (int x = 0; x < size - 1; x++)
                if (modules[y][x] == modules[y][x + 1]
                        && modules[y][x] == modules[y + 1][x]
                        && modules[y][x] == modules[y + 1][x + 1])
                    score += 3;
        // Finder-like patterns (1:1:3:1:1 with 4+ light modules on a side).
        for (int y = 0; y < size; y++) score += finderPenaltyLine(rowBits(y));
        for (int x = 0; x < size; x++) score += finderPenaltyLine(colBits(x));
        // Dark/light balance.
        int dark = 0;
        for (boolean[] row : modules)
            for (boolean m : row)
                if (m) dark++;
        int total = size * size;
        double percent = dark * 100.0 / total;
        int k = (int) Math.ceil(Math.abs(percent - 50) / 5) - 1;
        if (k > 0) score += k * 10;
        return score;
    }

    private int[] rowBits(int y) {
        int[] bits = new int[size + 8];
        for (int x = 0; x < size; x++) bits[x + 4] = modules[y][x] ? 1 : 0;
        return bits;
    }

    private int[] colBits(int x) {
        int[] bits = new int[size + 8];
        for (int y = 0; y < size; y++) bits[y + 4] = modules[y][x] ? 1 : 0;
        return bits;
    }

    private int finderPenaltyLine(int[] bits) {
        final int[] p1 = {0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1};
        final int[] p2 = {1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0};
        int score = 0;
        for (int i = 0; i + 11 <= bits.length; i++) {
            boolean m1 = true, m2 = true;
            for (int j = 0; j < 11; j++) {
                if (bits[i + j] != p1[j]) m1 = false;
                if (bits[i + j] != p2[j]) m2 = false;
            }
            if (m1 || m2) score += 40;
        }
        return score;
    }

    // ---------------------------------------------------------- bit buffer

    private static final class BitBuffer {
        private int[] words = new int[64];
        private int length = 0;

        void appendBits(int val, int len) {
            for (int i = len - 1; i >= 0; i--) {
                if (length == words.length * 32) {
                    int[] bigger = new int[words.length * 2];
                    System.arraycopy(words, 0, bigger, 0, words.length);
                    words = bigger;
                }
                if (((val >>> i) & 1) != 0)
                    words[length >>> 5] |= 1 << (31 - (length & 31));
                length++;
            }
        }

        int length() { return length; }

        byte[] getBytes() {
            byte[] out = new byte[(length + 7) / 8];
            for (int i = 0; i < length; i++) {
                if (((words[i >>> 5] >>> (31 - (i & 31))) & 1) != 0)
                    out[i >>> 3] |= (byte) (1 << (7 - (i & 7)));
            }
            return out;
        }
    }
}
