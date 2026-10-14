import { exampleSetup } from "prosemirror-example-setup";
import { keymap } from "prosemirror-keymap";
import { Schema } from "prosemirror-model";
import { schema as basicSchema } from "prosemirror-schema-basic";
import { addListNodes } from "prosemirror-schema-list";
import { EditorState } from "prosemirror-state";
import { goToNextCell, tableEditing, tableNodes } from "prosemirror-tables";
import { EditorView } from "prosemirror-view";
import "../../demo/basic/public/prosemirror.css";
import "../../demo/basic/public/editor.css";
import "./app.css";

const listSchema = new Schema({
  nodes: addListNodes(basicSchema.spec.nodes, "paragraph block*", "block"),
  marks: basicSchema.spec.marks,
});

const tableSchema = new Schema({
  nodes: addListNodes(basicSchema.spec.nodes, "paragraph block*", "block").append(
    tableNodes({ tableGroup: "block" }),
  ),
  marks: basicSchema.spec.marks,
});

const engine = document.querySelector("#engine");
const basicRoot = document.querySelector("#basic-root");
const advancedRoot = document.querySelector("#advanced-root");
const basicButton = document.querySelector("#mode-basic");
const advancedButton = document.querySelector("#mode-advanced");

function mountBasic() {
  const host = document.createElement("div");
  host.id = "editor";
  basicRoot.replaceChildren(host);
  const state = EditorState.create({
    schema: listSchema,
    plugins: exampleSetup({ schema: listSchema }),
  });
  const view = new EditorView(host, { state });
  window._proseMirrorView = view;
  window._proseMirrorState = view.state;
  return () => {
    view.destroy();
    if (window._proseMirrorView === view) {
      delete window._proseMirrorView;
      delete window._proseMirrorState;
    }
  };
}

function mountAdvanced() {
  const editors = document.createElement("div");
  editors.className = "editors";
  const paneA = document.createElement("div");
  paneA.className = "editor-pane";
  paneA.innerHTML = "<h2>Editor A</h2>";
  const hostA = document.createElement("div");
  hostA.id = "editor-a";
  paneA.append(hostA);
  const paneB = document.createElement("div");
  paneB.className = "editor-pane";
  paneB.innerHTML = "<h2>Editor B</h2>";
  const hostB = document.createElement("div");
  hostB.id = "editor-b";
  paneB.append(hostB);
  editors.append(paneA, paneB);
  const status = document.createElement("div");
  status.id = "status";
  advancedRoot.replaceChildren(editors, status);

  const stepsQueue = [];
  const views = [];

  function updateStatus() {
    const a = views.find((item) => item.label === "A");
    const b = views.find((item) => item.label === "B");
    if (!a || !b) return;
    status.textContent =
      `Collaboration active — Editor A length: ${a.view.state.doc.content.size}, ` +
      `Editor B length: ${b.view.state.doc.content.size}`;
  }

  function create(host, label) {
    const view = new EditorView(host, {
      state: EditorState.create({
        schema: tableSchema,
        plugins: [
          ...exampleSetup({ schema: tableSchema, menuBar: true }),
          tableEditing(),
          keymap({
            Tab: goToNextCell(1),
            "Shift-Tab": goToNextCell(-1),
          }),
        ],
      }),
      dispatchTransaction(tr) {
        view.updateState(view.state.apply(tr));
        if (tr.steps.length) {
          stepsQueue.push({ from: label, steps: tr.steps });
        }
        updateStatus();
      },
    });
    views.push({ view, label });
    return view;
  }

  const viewA = create(hostA, "A");
  const viewB = create(hostB, "B");
  const timer = window.setInterval(() => {
    while (stepsQueue.length) {
      const item = stepsQueue.shift();
      for (const entry of views) {
        if (entry.label === item.from) continue;
        let tr = entry.view.state.tr.setMeta("addToHistory", false);
        let modified = false;
        for (const step of item.steps) {
          const mapped = step.map(tr.mapping);
          if (mapped && !tr.maybeStep(mapped).failed) modified = true;
        }
        if (modified) entry.view.updateState(entry.view.state.apply(tr));
      }
    }
    updateStatus();
  }, 200);
  updateStatus();
  window._proseMirrorViewA = viewA;
  window._proseMirrorViewB = viewB;

  return () => {
    window.clearInterval(timer);
    viewA.destroy();
    viewB.destroy();
    if (window._proseMirrorViewA === viewA) {
      delete window._proseMirrorViewA;
      delete window._proseMirrorViewB;
    }
  };
}

function show(mode) {
  const advanced = mode === "advanced";
  basicRoot.hidden = advanced;
  advancedRoot.hidden = !advanced;
  basicButton.classList.toggle("is-on", !advanced);
  advancedButton.classList.toggle("is-on", advanced);
  basicButton.setAttribute("aria-pressed", String(!advanced));
  advancedButton.setAttribute("aria-pressed", String(advanced));
}

mountBasic();
mountAdvanced();
basicButton.addEventListener("click", () => show("basic"));
advancedButton.addEventListener("click", () => show("advanced"));
show("basic");

const wasmNote = "prosemirror-rs via WebAssembly — the same crate the Tauri 2.12.3 process links natively.";
engine.textContent = wasmNote;

if ("__TAURI_INTERNALS__" in window) {
  import("@tauri-apps/api/core")
    .then(({ invoke }) => Promise.all([invoke("engine_info"), invoke("self_test")]))
    .then(([info, report]) => {
      engine.textContent = `${info.name} ${info.version} inside Tauri ${info.tauri}. ${report.passed.length} native engine checks passed.`;
    })
    .catch((err) => {
      engine.textContent = err instanceof Error ? err.message : "Native engine check failed";
    });
}
