// A headend station's page opens on the headend view: the station says no
// mixer stands behind the page with the features `station` and `headend`
// in its own `core.info`. A mixer's page, or a station without a Flussonic
// config, opens as it always has.

import { wallStub } from "./wall-stub.js";
import { wait, until } from "./wall-help.js";
import { openIfHeadend } from "../panels/headend/station.js";
import { toggleHeadend } from "../panels/headend/view.js";

function saying(features) {
  const stub = wallStub({ n: 3 });
  const inner = stub.call;
  stub.call = (method, params) => (method === "core.info" ? Promise.resolve({ core: "godwinmix", features }) : inner(method, params));
  return stub;
}

export async function headendStationTests(test, eq, ok) {
  await test("a headend station's page opens on the headend view", async () => {
    eq(await openIfHeadend(saying(["api-v1", "headend", "rpc", "station"])), true);
    await until(() => document.querySelector(".he"));
    ok(document.querySelector(".he"), "the view is up");
    toggleHeadend();
    await wait(50);
    eq(document.querySelector(".he"), null);
  });
  await test("a mixer's page and a station without a Flussonic config open as before", async () => {
    eq(await openIfHeadend(saying(["api-v1", "mjpeg", "rpc"])), false);
    eq(await openIfHeadend(saying(["api-v1", "rpc", "station"])), false);
    eq(document.querySelector(".he"), null, "no view opened");
  });
}
