import Mexp from "math-expression-evaluator";

const MAX_EXPRESSION_LENGTH = 160;
const engine = new Mexp();

export interface Calculation {
  expression: string;
  result: string;
}

function normalizeExpression(input: string): string {
  let expression = input
    .trim()
    .replace(/[０-９]/g, (digit) =>
      String.fromCharCode(digit.charCodeAt(0) - 0xfee0),
    )
    .replace(/[＋]/g, "+")
    .replace(/[－−–—]/g, "-")
    .replace(/[＊×]/g, "*")
    .replace(/[／÷]/g, "/")
    .replace(/[＾]/g, "^")
    .replace(/[％]/g, "%")
    .replace(/[．]/g, ".")
    .replace(/[（]/g, "(")
    .replace(/[）]/g, ")")
    .replace(/[＝]/g, "=");

  if (expression.startsWith("=")) expression = expression.slice(1).trim();
  if (expression.endsWith("=")) expression = expression.slice(0, -1).trim();
  return expression.replace(/(\d|\))\s*[xXｘＸ]\s*(?=\d|\()/g, "$1*");
}

function hasBalancedParentheses(expression: string): boolean {
  let depth = 0;
  for (const character of expression) {
    if (character === "(") depth += 1;
    if (character === ")") depth -= 1;
    if (depth < 0) return false;
  }
  return depth === 0;
}

function formatResult(value: number): string {
  const normalized = Object.is(value, -0) ? 0 : value;
  const plain = String(normalized);
  if (plain.length <= 18) return plain;
  return normalized
    .toExponential(10)
    .replace(/\.0+(?=e)/, "")
    .replace(/(\.\d*?[1-9])0+(?=e)/, "$1");
}

export function calculateExpression(input: string): Calculation | null {
  const expression = normalizeExpression(input);
  if (!expression || expression.length > MAX_EXPRESSION_LENGTH) return null;
  if (!/^[\d+\-*/%^().\s]+$/.test(expression)) return null;
  if (!hasBalancedParentheses(expression)) return null;

  const numbers = expression.match(/(?:\d+(?:\.\d*)?|\.\d+)/g);
  if (!numbers || numbers.length < 2 || !/[+\-*/^%]/.test(expression)) return null;

  try {
    const value = engine.eval(expression.replace(/%/g, " Mod "));
    if (!Number.isFinite(value)) return null;
    return { expression, result: formatResult(value) };
  } catch {
    return null;
  }
}
