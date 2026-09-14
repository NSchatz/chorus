// The base tree's script. It builds a row and carries no style of its own,
// which is what keeps the stylesheet sources the whole surface.

function render(root, value) {
  const cell = document.createElement("p");
  cell.className = "figure";
  cell.textContent = String(value);
  root.replaceChildren(cell);
}

document.addEventListener("DOMContentLoaded", function () {
  render(document.querySelector("main"), 0);
});
