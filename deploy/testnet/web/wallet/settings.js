// The network's settings (config.json) as the page needs them. A file that parses but lacks what the page reads
// (`{}`, `null`, a token without decimals) is refused here, so the page says it could not read the network's settings
// and stops, instead of failing later on a missing value.
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/** Returns `c` when it holds `rpc`, `token.decimals` and `token.symbol`; throws naming the first that is missing. */
export function checkConfig(c) {
  if (!isObject(c)) throw new Error('not a settings object');
  if (typeof c.rpc !== 'string' || !c.rpc.trim()) throw new Error('rpc is missing');
  if (!isObject(c.token)) throw new Error('token is missing');
  const d = c.token.decimals;
  if (!Number.isInteger(d) || d < 0 || d > 36) throw new Error('token.decimals is not a whole number from 0 to 36');
  if (typeof c.token.symbol !== 'string' || !c.token.symbol.trim()) throw new Error('token.symbol is missing');
  return c;
}
