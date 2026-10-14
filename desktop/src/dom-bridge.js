// ESM bridge over the package's CommonJS DOM helpers.
import * as toDom from "../../node/npm/to-dom.js";
import * as fromDom from "../../node/npm/from-dom.js";

function pick(mod, name) {
  if (typeof mod[name] === "function") return mod[name];
  if (mod.default && typeof mod.default[name] === "function") return mod.default[name];
  throw new Error(`prosemirror-rs DOM helper ${name} is missing`);
}

export class ReplaceError extends Error {
  constructor(message) {
    super(message);
    this.name = "ReplaceError";
  }
}

export function createDOMTypes(binding) {
  return {
    ReplaceError,
    DOMSerializer: pick(toDom, "createDOMSerializer")(binding),
    DOMParser: pick(fromDom, "createDOMParser")(binding),
  };
}
