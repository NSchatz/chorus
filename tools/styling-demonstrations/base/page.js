// The base demonstration tree's script: it changes what the page SAYS and never
// what it looks like. A class is the sanctioned route from the script to a
// colour or a length, because a class resolves to a rule in a stylesheet source
// and that is where the styling rules can read it.

export function showCount(node, count) {
  node.textContent = `${count} zones`;
  node.classList.toggle("card-empty", count === 0);
}
