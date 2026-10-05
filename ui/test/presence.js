// Several people on one mixer: the presence helpers, the conflict sentence,
// and a drag that does not snap to somebody else's echo.

import { others, editing, nameOf, sentence } from "../kits/protocol/presence.js";
import { Prediction } from "../kits/protocol/predict.js";
import { whatChanged } from "../shell/conflicts.js";

const LIST = {
  clients: [
    { client_id: "open.t1", token: "open", device: "Windows Edge", since_ms: 1, you: true, scene: "s-1" },
    { client_id: "open.phone", token: "open", label: "Sam", device: "iPhone Safari", since_ms: 2, scene: "s-1" },
    { client_id: "open.s3", token: "open", device: "", since_ms: 3 },
  ],
};

export function presenceTests(test, eq, ok) {
  test("presence: the others are everybody but this page", () => {
    eq(others(LIST).map((c) => c.client_id), ["open.phone", "open.s3"]);
    eq(others(null), []);
  });

  test("presence: only the others in the same scene are marked in it", () => {
    eq(editing(LIST, "s-1").map((c) => c.client_id), ["open.phone"]);
    eq(editing(LIST, "s-2"), []);
    eq(editing(LIST, null), []);
  });

  test("presence: a client is named by its label and device, its id only as a last resort", () => {
    eq(nameOf(LIST.clients[1]), "Sam (iPhone Safari)");
    eq(nameOf(LIST.clients[0]), "Windows Edge");
    eq(nameOf(LIST.clients[2]), "open.s3");
    eq(sentence(others(LIST)), "Sam (iPhone Safari) and open.s3");
    eq(sentence([1, 2, 3].map(() => LIST.clients[2])), "3 others");
  });

  test("predict: somebody else's echo never settles or replaces our move", () => {
    const p = new Prediction();
    p.predict("box", { opacity: 0.5 });
    ok(!p.accepts("box", 99, false), "another client's number was read as ours");
    ok(p.accepts("box", 1, true), "our own echo should be accepted");
    ok(p.accepts("other", 0, false), "an item we are not moving takes anybody's change");
  });

  test("conflicts: a refusal reads as who changed what", () => {
    const err = {
      data: {
        conflict: "undo",
        conflicts: [
          { kind: "item", name: "stage", changed_by: "open.phone", who: "Sam (iPhone Safari)", gone: false },
          { kind: "scene", name: "wide", changed_by: "open.s3", gone: true },
        ],
      },
    };
    eq(whatChanged(err), 'item "stage" (changed by Sam (iPhone Safari)), scene "wide" (removed by open.s3)');
    eq(whatChanged({}), "this scene");
  });
}
