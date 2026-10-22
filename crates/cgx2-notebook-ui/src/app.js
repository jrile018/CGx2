"use strict";
const token = document.querySelector('meta[name="cgx2-token"]').content;
const cells = document.getElementById("cells");
const notice = document.getElementById("notice");
let current = null;
let busy = false;
const pending = new Map();
function el(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
}
function render(data) {
  const saveName = document.getElementById("save-name").value;
  current = data;
  cells.replaceChildren();
  for (const cell of data.cells) {
    const section = el("article", "cell");
    section.append(el("h2", "", `${cell.kind} · ${cell.id}`));
    section.append(el("p", "status", `Status: ${cell.status}`));
    if (cell.kind === "code") {
      const editor = el("textarea");
      editor.value = pending.get(cell.id) ?? cell.source;
      editor.setAttribute("aria-label", `Code for ${cell.id}`);
      editor.addEventListener("input", () => {
        if (editor.value === cell.source) pending.delete(cell.id);
        else pending.set(cell.id, editor.value);
        notice.textContent = pending.size ? "Unapplied source edits. Apply them before running or saving." : "";
      });
      section.append(editor);
      const apply = el("button", "", "Apply edit");
      apply.addEventListener("click", () => command(null, null));
      section.append(apply);
    } else section.append(el("pre", "", cell.source));
    for (const output of cell.outputs) section.append(el("pre", "output", output.kind === "text" ? output.body : `[cached asset ${output.body}]`));
    cells.append(section);
  }
  notice.textContent = data.needsRestart ? "Source changed. Restart and run all before executing another cell." : (data.lastError || "");
  document.getElementById("save-name").placeholder = data.inputName.replace(/\.cgnb$/, "-saved.cgnb");
  document.getElementById("save-name").value = saveName;
}
async function send(path, body) {
  const response = await fetch(path, {method:"POST", headers:{"Content-Type":"application/json", "X-CGX2-Token":token}, body:JSON.stringify(body), cache:"no-store"});
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "Request failed");
  return data;
}
async function command(path, body) {
  if (busy) return;
  busy = true;
  for (const button of document.querySelectorAll("button")) button.disabled = true;
  try {
    let data = current;
    for (const [id, source] of pending) {
      data = await send("/api/edit", {id, source});
      pending.delete(id);
    }
    if (path) data = await send(path, body);
    notice.className = "";
    render(data);
    if (path === "/api/save") notice.textContent = `Saved as ${body.name}`;
  } catch (error) {
    try { await refresh(); } catch (_) { /* keep the original error */ }
    notice.textContent = `${error}`;
    notice.className = "error";
  }
  finally { busy = false; for (const button of document.querySelectorAll("button")) button.disabled = false; }
}
async function refresh() {
  const response = await fetch("/api/state", {cache:"no-store"});
  render(await response.json());
}
document.getElementById("next").addEventListener("click", () => command("/api/run-next", {}));
document.getElementById("restart").addEventListener("click", () => command("/api/restart", {}));
document.getElementById("save").addEventListener("click", () => {
  const input = document.getElementById("save-name");
  command("/api/save", {name:input.value || input.placeholder, overwrite:document.getElementById("overwrite").checked});
});
refresh().catch(error => { notice.textContent = `Connection error: ${error}`; });
