"use strict";
const token = document.querySelector('meta[name="cgx2-token"]').content;
const cells = document.getElementById("cells");
const notice = document.getElementById("notice");
let current = null;
let busy = false;
const pending = new Map();
let imageUrls = [];
function el(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
}
function render(data) {
  const saveName = document.getElementById("save-name").value;
  for (const url of imageUrls) URL.revokeObjectURL(url);
  imageUrls = [];
  current = data;
  cells.replaceChildren();
  for (const cell of data.cells) {
    const section = el("article", "cell");
    section.append(el("h2", "", `${cell.kind} · ${cell.id}`));
    section.append(el("p", "status", `Status: ${cell.status}`));
    if (cell.provenance) section.append(el("p", "provenance", `Run #${cell.provenance.ordinal} (${cell.provenance.result}) · ${cell.provenance.evaluator} · source ${cell.provenance.sourceSha256} · prefix ${cell.provenance.prefixSha256}`));
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
      const run = el("button", "", "Run this cell");
      run.setAttribute("aria-label", `Run cell ${cell.id}`);
      run.addEventListener("click", () => command("/api/run-cell", {id:cell.id}));
      section.append(run);
    } else section.append(el("pre", "", cell.source));
    for (const output of cell.outputs) {
      const label = cell.status === "current" ? "Current output" : `${cell.status} output (saved observation)`;
      if (output.kind === "text") section.append(el("pre", "output", `${label}: ${output.body}`));
      else {
        const figure = el("figure", "output");
        figure.append(el("figcaption", "", `${label}: ${output.mime} · ${output.size} bytes · SHA-256 ${output.body}`));
        const img = el("img");
        img.alt = `Saved image output for ${cell.id}`;
        figure.append(img);
        section.append(figure);
        fetch("/api/asset", {method:"POST", headers:{"Content-Type":"application/json", "X-CGX2-Token":token},
          body:JSON.stringify({sha256:output.body}), cache:"no-store"})
          .then(response => { if (!response.ok) throw new Error("asset unavailable"); return response.blob(); })
          .then(blob => { if (img.isConnected) { const url = URL.createObjectURL(blob); imageUrls.push(url); img.src = url; } })
          .catch(() => { if (img.isConnected) img.replaceWith(el("span", "error", "Saved image unavailable")); });
      }
    }
    cells.append(section);
  }
  notice.textContent = [data.lastError, data.needsRestart ? "Source changed or a rerun failed. Run this cell or Restart and run all before Run next." : ""].filter(Boolean).join(" ");
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
    if (path === "/api/export") notice.textContent = `Exported portable notebook as ${body.name}`;
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
document.getElementById("export").addEventListener("click", () => {
  const input = document.getElementById("save-name");
  const name = input.value || (current ? current.inputName.replace(/\.cgnb$/, "-portable.cgnb") : "portable.cgnb");
  command("/api/export", {name, overwrite:document.getElementById("overwrite").checked});
});
refresh().catch(error => { notice.textContent = `Connection error: ${error}`; });
