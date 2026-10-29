const VERSIONS = [
  { version: 1, size: 21, data: 19, ec: 7, align: [] as number[] },
  { version: 2, size: 25, data: 34, ec: 10, align: [18] },
  { version: 3, size: 29, data: 55, ec: 15, align: [22] },
  { version: 4, size: 33, data: 80, ec: 20, align: [26] },
  { version: 5, size: 37, data: 108, ec: 26, align: [30] },
  { version: 6, size: 41, data: 136, ec: 36, align: [34] },
] as const;

const EXP = new Uint8Array(512);
const LOG = new Uint8Array(256);

(function initGf() {
  let value = 1;
  for (let i = 0; i < 255; i += 1) {
    EXP[i] = value;
    LOG[value] = i;
    value <<= 1;
    if (value & 0x100) value ^= 0x11d;
  }
  for (let i = 255; i < 512; i += 1) EXP[i] = EXP[i - 255];
})();

function gfMul(a: number, b: number) {
  if (a === 0 || b === 0) return 0;
  return EXP[LOG[a] + LOG[b]];
}

function rsGenerator(degree: number) {
  const poly = [1];
  for (let i = 0; i < degree; i += 1) {
    const next = new Array(poly.length + 1).fill(0);
    for (let j = 0; j < poly.length; j += 1) {
      next[j] ^= gfMul(poly[j], EXP[i]);
      next[j + 1] ^= poly[j];
    }
    poly.length = 0;
    poly.push(...next);
  }
  return poly;
}

function rsEncode(data: number[], ecCount: number) {
  const gen = rsGenerator(ecCount);
  const res = new Array(ecCount).fill(0);
  for (const byte of data) {
    const factor = byte ^ res[0];
    res.shift();
    res.push(0);
    if (factor === 0) continue;
    for (let i = 0; i < gen.length - 1; i += 1) {
      res[i] ^= gfMul(gen[i + 1], factor);
    }
  }
  return res;
}

function chooseVersion(byteLength: number) {
  for (const spec of VERSIONS) {
    const capacity = spec.data - 2;
    if (byteLength <= capacity) return spec;
  }
  return VERSIONS[VERSIONS.length - 1];
}

function encodeData(text: string, spec: (typeof VERSIONS)[number]) {
  const bytes = Array.from(new TextEncoder().encode(text));
  const bits: number[] = [];
  const push = (value: number, length: number) => {
    for (let i = length - 1; i >= 0; i -= 1) bits.push((value >> i) & 1);
  };

  push(0b0100, 4);
  push(bytes.length, 8);
  for (const byte of bytes) push(byte, 8);

  const capacityBits = spec.data * 8;
  const terminator = Math.min(4, capacityBits - bits.length);
  push(0, terminator);
  while (bits.length % 8 !== 0) bits.push(0);

  const data: number[] = [];
  for (let i = 0; i < bits.length; i += 8) {
    let byte = 0;
    for (let j = 0; j < 8; j += 1) byte = (byte << 1) | bits[i + j];
    data.push(byte);
  }

  const pads = [0xec, 0x11];
  let pad = 0;
  while (data.length < spec.data) {
    data.push(pads[pad % 2]);
    pad += 1;
  }

  return data.concat(rsEncode(data, spec.ec));
}

function isReserved(row: number, col: number, size: number, align: readonly number[]) {
  if (row <= 8 && col <= 8) return true;
  if (row <= 8 && col >= size - 8) return true;
  if (row >= size - 8 && col <= 8) return true;
  if (row === 6 || col === 6) return true;

  for (const ar of [6, ...align]) {
    for (const ac of [6, ...align]) {
      if (ar === 6 && ac === 6) continue;
      if (ar === 6 && ac === size - 7) continue;
      if (ar === size - 7 && ac === 6) continue;
      if (Math.abs(row - ar) <= 2 && Math.abs(col - ac) <= 2) return true;
    }
  }
  return false;
}

function setFinder(matrix: number[][], row: number, col: number) {
  for (let r = -1; r <= 7; r += 1) {
    for (let c = -1; c <= 7; c += 1) {
      const rr = row + r;
      const cc = col + c;
      if (rr < 0 || cc < 0 || rr >= matrix.length || cc >= matrix.length) continue;
      const on =
        r >= 0 &&
        r <= 6 &&
        c >= 0 &&
        c <= 6 &&
        (r === 0 || r === 6 || c === 0 || c === 6 || (r >= 2 && r <= 4 && c >= 2 && c <= 4));
      matrix[rr][cc] = on ? 1 : 0;
    }
  }
}

function setAlignment(matrix: number[][], row: number, col: number) {
  for (let r = -2; r <= 2; r += 1) {
    for (let c = -2; c <= 2; c += 1) {
      matrix[row + r][col + c] = r === -2 || r === 2 || c === -2 || c === 2 || (r === 0 && c === 0) ? 1 : 0;
    }
  }
}

function maskBit(mask: number, row: number, col: number) {
  switch (mask) {
    case 0:
      return (row + col) % 2 === 0;
    case 1:
      return row % 2 === 0;
    case 2:
      return col % 3 === 0;
    case 3:
      return (row + col) % 3 === 0;
    case 4:
      return (Math.floor(row / 2) + Math.floor(col / 3)) % 2 === 0;
    case 5:
      return ((row * col) % 2) + ((row * col) % 3) === 0;
    case 6:
      return (((row * col) % 2) + ((row * col) % 3)) % 2 === 0;
    default:
      return (((row + col) % 2) + ((row * col) % 3)) % 2 === 0;
  }
}

function formatBits(mask: number) {
  const data = (0b01 << 3) | mask;
  let bits = data << 10;
  const gen = 0b10100110111;
  for (let i = 14; i >= 10; i -= 1) {
    if ((bits >> i) & 1) bits ^= gen << (i - 10);
  }
  return (data << 10 | bits) ^ 0b101010000010010;
}

function applyFormat(matrix: number[][], mask: number) {
  const bits = formatBits(mask);
  const size = matrix.length;
  const positions: Array<[number, number]> = [];

  for (let i = 0; i <= 5; i += 1) positions.push([i, 8]);
  positions.push([7, 8], [8, 8], [8, 7]);
  for (let i = 5; i >= 0; i -= 1) positions.push([8, i]);

  const other: Array<[number, number]> = [];
  for (let i = 0; i <= 7; i += 1) other.push([8, size - 1 - i]);
  for (let i = 0; i <= 6; i += 1) other.push([size - 7 + i, 8]);

  for (let i = 0; i < 15; i += 1) {
    const bit = (bits >> (14 - i)) & 1;
    matrix[positions[i][0]][positions[i][1]] = bit;
    matrix[other[i][0]][other[i][1]] = bit;
  }
}

function buildMatrix(text: string) {
  const bytes = new TextEncoder().encode(text);
  const spec = chooseVersion(bytes.length);
  const size = spec.size;
  const codewords = encodeData(text, spec);
  const matrix = Array.from({ length: size }, () => Array<number>(size).fill(0));
  const reserved = Array.from({ length: size }, () => Array<boolean>(size).fill(false));

  const markReserved = () => {
    for (let r = 0; r < size; r += 1) {
      for (let c = 0; c < size; c += 1) {
        reserved[r][c] = isReserved(r, c, size, spec.align);
      }
    }
  };

  setFinder(matrix, 0, 0);
  setFinder(matrix, 0, size - 7);
  setFinder(matrix, size - 7, 0);

  for (const ar of [6, ...spec.align]) {
    for (const ac of [6, ...spec.align]) {
      if (ar === 6 && ac === 6) continue;
      if (ar === 6 && ac === size - 7) continue;
      if (ar === size - 7 && ac === 6) continue;
      if (spec.align.length === 0) continue;
      setAlignment(matrix, ar, ac);
    }
  }

  for (let i = 8; i < size - 8; i += 1) {
    matrix[6][i] = i % 2 === 0 ? 1 : 0;
    matrix[i][6] = i % 2 === 0 ? 1 : 0;
  }

  matrix[size - 8][8] = 1;
  markReserved();

  const bits: number[] = [];
  for (const word of codewords) {
    for (let i = 7; i >= 0; i -= 1) bits.push((word >> i) & 1);
  }

  let bitIndex = 0;
  let upward = true;
  for (let col = size - 1; col > 0; col -= 2) {
    if (col === 6) col -= 1;
    for (let i = 0; i < size; i += 1) {
      const row = upward ? size - 1 - i : i;
      for (const c of [col, col - 1]) {
        if (reserved[row][c]) continue;
        matrix[row][c] = bits[bitIndex] ?? 0;
        bitIndex += 1;
      }
    }
    upward = !upward;
  }

  let best = matrix;
  let bestScore = Infinity;
  for (let mask = 0; mask < 8; mask += 1) {
    const masked = matrix.map((row) => row.slice());
    for (let r = 0; r < size; r += 1) {
      for (let c = 0; c < size; c += 1) {
        if (!reserved[r][c] && maskBit(mask, r, c)) masked[r][c] ^= 1;
      }
    }
    applyFormat(masked, mask);
    const score = penalty(masked);
    if (score < bestScore) {
      bestScore = score;
      best = masked;
    }
  }

  return best;
}

function penalty(matrix: number[][]) {
  const size = matrix.length;
  let score = 0;

  for (let r = 0; r < size; r += 1) {
    let run = 1;
    for (let c = 1; c <= size; c += 1) {
      if (c < size && matrix[r][c] === matrix[r][c - 1]) {
        run += 1;
      } else {
        if (run >= 5) score += 3 + (run - 5);
        run = 1;
      }
    }
  }

  for (let c = 0; c < size; c += 1) {
    let run = 1;
    for (let r = 1; r <= size; r += 1) {
      if (r < size && matrix[r][c] === matrix[r - 1][c]) {
        run += 1;
      } else {
        if (run >= 5) score += 3 + (run - 5);
        run = 1;
      }
    }
  }

  for (let r = 0; r < size - 1; r += 1) {
    for (let c = 0; c < size - 1; c += 1) {
      const v = matrix[r][c];
      if (matrix[r][c + 1] === v && matrix[r + 1][c] === v && matrix[r + 1][c + 1] === v) {
        score += 3;
      }
    }
  }

  return score;
}

export function QrCode({ value }: { value: string }) {
  const matrix = buildMatrix(value);
  const size = matrix.length;
  const quiet = 2;
  const view = size + quiet * 2;
  const modules = matrix.flatMap((row, y) =>
    row.flatMap((on, x) =>
      on ? [`${x + quiet} ${y + quiet}h1v1h-1z`] : [],
    ),
  );

  return (
    <svg
      viewBox={`0 0 ${view} ${view}`}
      role="img"
      aria-label={`QR code for ${value}`}
      className="h-full w-full"
    >
      <rect width={view} height={view} fill="#fff" />
      <path d={modules.map((item) => `M${item}`).join("")} fill="#100b02" />
    </svg>
  );
}
