export class OutOfBalance extends Error {
  constructor(delta: number) {
    super(`QUILLON-409: ledger out of balance by ${delta}`);
  }
}
