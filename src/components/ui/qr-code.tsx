interface QrCodeProps {
  size?: number;
  value: string;
}

const VERSION_2_SIZE = 25;
const DATA_CODEWORDS = 34;
const ECC_CODEWORDS = 10;
const FORMAT_BITS_L_MASK_0 = 0x77c4;
const QUIET_ZONE = 4;

function appendBits(bits: number[], value: number, length: number) {
  for (let shift = length - 1; shift >= 0; shift -= 1) {
    bits.push((value >>> shift) & 1);
  }
}

function buildDataCodewords(value: string): number[] {
  const bytes = Array.from(new TextEncoder().encode(value));
  if (bytes.length > 32) {
    throw new Error("Mobile Preview QR payload is too long for the fixed generator.");
  }

  const bits: number[] = [];
  appendBits(bits, 0b0100, 4);
  appendBits(bits, bytes.length, 8);
  bytes.forEach((byte) => appendBits(bits, byte, 8));

  const capacityBits = DATA_CODEWORDS * 8;
  appendBits(bits, 0, Math.min(4, capacityBits - bits.length));
  while (bits.length % 8 !== 0) {
    bits.push(0);
  }

  const codewords: number[] = [];
  for (let index = 0; index < bits.length; index += 8) {
    let byte = 0;
    for (let offset = 0; offset < 8; offset += 1) {
      byte = (byte << 1) | bits[index + offset]!;
    }
    codewords.push(byte);
  }

  const pads = [0xec, 0x11];
  while (codewords.length < DATA_CODEWORDS) {
    codewords.push(pads[codewords.length % 2]!);
  }

  return codewords;
}

function buildGaloisTables() {
  const exp = new Array<number>(512).fill(0);
  const log = new Array<number>(256).fill(0);
  let value = 1;

  for (let index = 0; index < 255; index += 1) {
    exp[index] = value;
    log[value] = index;
    value <<= 1;
    if ((value & 0x100) !== 0) {
      value ^= 0x11d;
    }
  }

  for (let index = 255; index < exp.length; index += 1) {
    exp[index] = exp[index - 255]!;
  }

  return { exp, log };
}

const GF_TABLES = buildGaloisTables();

function gfMultiply(left: number, right: number) {
  if (left === 0 || right === 0) {
    return 0;
  }

  return GF_TABLES.exp[GF_TABLES.log[left]! + GF_TABLES.log[right]!]!;
}

function polynomialMultiply(left: number[], right: number[]) {
  const result = new Array<number>(left.length + right.length - 1).fill(0);
  for (let leftIndex = 0; leftIndex < left.length; leftIndex += 1) {
    for (let rightIndex = 0; rightIndex < right.length; rightIndex += 1) {
      result[leftIndex + rightIndex] ^=
        gfMultiply(left[leftIndex]!, right[rightIndex]!);
    }
  }
  return result;
}

function buildGeneratorPolynomial(degree: number) {
  let generator = [1];
  for (let power = 0; power < degree; power += 1) {
    generator = polynomialMultiply(generator, [1, GF_TABLES.exp[power]!]);
  }
  return generator;
}

const GENERATOR_POLYNOMIAL = buildGeneratorPolynomial(ECC_CODEWORDS);

function buildErrorCorrectionCodewords(data: number[]) {
  const remainder = new Array<number>(ECC_CODEWORDS).fill(0);

  data.forEach((byte) => {
    const factor = byte ^ remainder[0]!;
    remainder.shift();
    remainder.push(0);

    GENERATOR_POLYNOMIAL.slice(1).forEach((coefficient, index) => {
      remainder[index] ^= gfMultiply(coefficient, factor);
    });
  });

  return remainder;
}

function drawFinder(matrix: boolean[][], reserved: boolean[][], left: number, top: number) {
  for (let dy = -1; dy <= 7; dy += 1) {
    for (let dx = -1; dx <= 7; dx += 1) {
      const x = left + dx;
      const y = top + dy;
      if (x < 0 || y < 0 || x >= VERSION_2_SIZE || y >= VERSION_2_SIZE) {
        continue;
      }

      const inside = dx >= 0 && dx <= 6 && dy >= 0 && dy <= 6;
      const isBlack =
        inside &&
        (dx === 0 ||
          dx === 6 ||
          dy === 0 ||
          dy === 6 ||
          ((dx >= 2 && dx <= 4) && (dy >= 2 && dy <= 4)));
      matrix[y]![x] = isBlack;
      reserved[y]![x] = true;
    }
  }
}

function drawAlignment(matrix: boolean[][], reserved: boolean[][], centerX: number, centerY: number) {
  for (let dy = -2; dy <= 2; dy += 1) {
    for (let dx = -2; dx <= 2; dx += 1) {
      const x = centerX + dx;
      const y = centerY + dy;
      const distance = Math.max(Math.abs(dx), Math.abs(dy));
      matrix[y]![x] = distance !== 1;
      reserved[y]![x] = true;
    }
  }
}

function setFunctionModule(
  matrix: boolean[][],
  reserved: boolean[][],
  x: number,
  y: number,
  value: boolean,
) {
  matrix[y]![x] = value;
  reserved[y]![x] = true;
}

function buildMatrix(value: string) {
  const matrix = Array.from({ length: VERSION_2_SIZE }, () =>
    new Array<boolean>(VERSION_2_SIZE).fill(false),
  );
  const reserved = Array.from({ length: VERSION_2_SIZE }, () =>
    new Array<boolean>(VERSION_2_SIZE).fill(false),
  );

  drawFinder(matrix, reserved, 0, 0);
  drawFinder(matrix, reserved, VERSION_2_SIZE - 7, 0);
  drawFinder(matrix, reserved, 0, VERSION_2_SIZE - 7);
  drawAlignment(matrix, reserved, 18, 18);

  for (let index = 8; index < VERSION_2_SIZE - 8; index += 1) {
    setFunctionModule(matrix, reserved, index, 6, index % 2 === 0);
    setFunctionModule(matrix, reserved, 6, index, index % 2 === 0);
  }

  setFunctionModule(matrix, reserved, 8, VERSION_2_SIZE - 8, true);

  for (let index = 0; index < 9; index += 1) {
    if (index !== 6) {
      reserved[8]![index] = true;
      reserved[index]![8] = true;
    }
  }
  for (let index = VERSION_2_SIZE - 8; index < VERSION_2_SIZE; index += 1) {
    reserved[8]![index] = true;
    reserved[index]![8] = true;
  }

  const dataCodewords = buildDataCodewords(value);
  const eccCodewords = buildErrorCorrectionCodewords(dataCodewords);
  const allCodewords = [...dataCodewords, ...eccCodewords];
  const dataBits: number[] = [];
  allCodewords.forEach((codeword) => appendBits(dataBits, codeword, 8));

  let bitIndex = 0;
  let upward = true;
  for (let right = VERSION_2_SIZE - 1; right >= 1; right -= 2) {
    if (right === 6) {
      right -= 1;
    }

    for (let offset = 0; offset < VERSION_2_SIZE; offset += 1) {
      const y = upward ? VERSION_2_SIZE - 1 - offset : offset;

      for (let columnOffset = 0; columnOffset < 2; columnOffset += 1) {
        const x = right - columnOffset;
        if (reserved[y]![x]) {
          continue;
        }

        const bit = dataBits[bitIndex] === 1;
        const masked = ((x + y) & 1) === 0 ? !bit : bit;
        matrix[y]![x] = masked;
        bitIndex += 1;
      }
    }

    upward = !upward;
  }

  for (let bit = 0; bit <= 5; bit += 1) {
    setFunctionModule(matrix, reserved, 8, bit, ((FORMAT_BITS_L_MASK_0 >>> bit) & 1) !== 0);
  }
  setFunctionModule(matrix, reserved, 8, 7, ((FORMAT_BITS_L_MASK_0 >>> 6) & 1) !== 0);
  setFunctionModule(matrix, reserved, 8, 8, ((FORMAT_BITS_L_MASK_0 >>> 7) & 1) !== 0);
  setFunctionModule(matrix, reserved, 7, 8, ((FORMAT_BITS_L_MASK_0 >>> 8) & 1) !== 0);
  for (let bit = 9; bit < 15; bit += 1) {
    setFunctionModule(matrix, reserved, 14 - bit, 8, ((FORMAT_BITS_L_MASK_0 >>> bit) & 1) !== 0);
  }

  for (let bit = 0; bit < 8; bit += 1) {
    setFunctionModule(
      matrix,
      reserved,
      VERSION_2_SIZE - 1 - bit,
      8,
      ((FORMAT_BITS_L_MASK_0 >>> bit) & 1) !== 0,
    );
  }
  for (let bit = 8; bit < 15; bit += 1) {
    setFunctionModule(
      matrix,
      reserved,
      8,
      VERSION_2_SIZE - 15 + bit,
      ((FORMAT_BITS_L_MASK_0 >>> bit) & 1) !== 0,
    );
  }

  return matrix;
}

function buildSvgPath(modules: boolean[][]) {
  const commands: string[] = [];
  modules.forEach((row, y) => {
    row.forEach((cell, x) => {
      if (!cell) {
        return;
      }

      const drawX = x + QUIET_ZONE;
      const drawY = y + QUIET_ZONE;
      commands.push(`M${drawX} ${drawY}h1v1H${drawX}z`);
    });
  });
  return commands.join("");
}

export function QrCode({ size = 220, value }: QrCodeProps) {
  const modules = buildMatrix(value);
  const viewBoxSize = VERSION_2_SIZE + QUIET_ZONE * 2;
  const path = buildSvgPath(modules);

  return (
    <svg
      aria-label="QR code"
      role="img"
      viewBox={`0 0 ${viewBoxSize} ${viewBoxSize}`}
      width={size}
      height={size}
    >
      <rect width={viewBoxSize} height={viewBoxSize} fill="#f8fbff" rx="2" />
      <path d={path} fill="#0c1826" />
    </svg>
  );
}
