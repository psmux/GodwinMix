// Source setup is fetched when an operator opens it or drops a URI.
import { lazyAction } from "./lazy-action.js";

export const openPicker = lazyAction(() => import("./picker.js").then(m => m.openPicker), "Open source setup");
export const openForm = lazyAction(() => import("./picker.js").then(m => m.openForm), "Open source settings");
export const pickFromDrop = lazyAction(() => import("./picker.js").then(m => m.pickFromDrop), "Open dropped source");
