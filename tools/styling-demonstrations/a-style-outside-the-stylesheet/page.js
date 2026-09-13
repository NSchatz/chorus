// A copy of tools/styling-demonstrations/base/page.js breaking the same one
// rule from the other side: the script paints, instead of naming a class and
// leaving the paint to a rule in a stylesheet source.

export function showCount(node, count) {
  node.textContent = `${count} zones`;
  node.style.color = "#3a4653";
  node.style.setProperty("--card-pad", "10px");
}
