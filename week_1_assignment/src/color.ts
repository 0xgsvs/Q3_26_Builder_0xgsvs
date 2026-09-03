export const c = {
  dim: (s: unknown) => `\x1b[2m${s}\x1b[22m`,
  green: (s: unknown) => `\x1b[32m${s}\x1b[39m`,
  cyan: (s: unknown) => `\x1b[36m${s}\x1b[39m`,
  magenta: (s: unknown) => `\x1b[35m${s}\x1b[39m`,
  yellow: (s: unknown) => `\x1b[33m${s}\x1b[39m`,
  red: (s: unknown) => `\x1b[31m${s}\x1b[39m`,
  bold: (s: unknown) => `\x1b[1m${s}\x1b[22m`,
};

export default c;
