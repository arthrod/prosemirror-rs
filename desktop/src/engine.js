// Browser entry. Namespace-import the CommonJS patches so the bundler does
// not rewrite them into a shape that breaks the wasm glue.

import init, * as wasmModule from "../../node/npm/wasm/prosemirror_rs_wasm.js";
import * as patch from "../../node/npm/patch.js";
import * as dom from "./dom-bridge.js";

await init();

const binding = { ...wasmModule };
patch.patchStatics(binding);
const domTypes = dom.createDOMTypes(binding);

export const Schema = binding.Schema;
export const Node = binding.Node;
export const NodeType = binding.NodeType;
export const Fragment = binding.Fragment;
export const Slice = binding.Slice;
export const ResolvedPos = binding.ResolvedPos;
export const NodeRange = binding.NodeRange;
export const Mark = binding.Mark;
export const MarkType = binding.MarkType;
export const ContentMatch = binding.ContentMatch;
export const StepMap = binding.StepMap;
export const MapResult = binding.MapResult;
export const Mapping = binding.Mapping;
export const Step = binding.Step;
export const Transform = binding.Transform;
export const liftTarget = binding.liftTarget;
export const findWrapping = binding.findWrapping;
export const canSplit = binding.canSplit;
export const canJoin = binding.canJoin;
export const joinPoint = binding.joinPoint;
export const insertPoint = binding.insertPoint;
export const dropPoint = binding.dropPoint;
export const contentMatchParse = binding.contentMatchParse;
export const ReplaceStep = binding.ReplaceStep;
export const ReplaceAroundStep = binding.ReplaceAroundStep;
export const AddMarkStep = binding.AddMarkStep;
export const RemoveMarkStep = binding.RemoveMarkStep;
export const AddNodeMarkStep = binding.AddNodeMarkStep;
export const RemoveNodeMarkStep = binding.RemoveNodeMarkStep;
export const AttrStep = binding.AttrStep;
export const DocAttrStep = binding.DocAttrStep;
export const replaceStep = binding.replaceStep;
export const ReplaceError = domTypes.ReplaceError;
export const DOMSerializer = domTypes.DOMSerializer;
export const DOMParser = domTypes.DOMParser;
export const setRawSpec = patch.setRawSpec;
export const getRawSpec = patch.getRawSpec;
