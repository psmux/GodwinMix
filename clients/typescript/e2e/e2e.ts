// One end to end run against a live core.
//
//     node e2e/e2e.ts http://127.0.0.1:8080 TOKEN
//
// Connect, subscribe, receive the snapshot and the flush, add a test source,
// take it, see `event/program.took`, disconnect. Every step prints ok, or the
// script exits non zero saying what failed.
//
// `clients/typescript/e2e/run.sh` starts a core the way dev/smoke.sh does and
// runs this against it.

import { connect, UI_EVENTS } from "../src/index.ts";

const base = process.argv[2] || "http://127.0.0.1:8080";
const token = process.argv[3] || null;
const ID = "e2e-typescript";

let failed = 0;

async function step<T>(name: string, work: () => Promise<T>): Promise<T> {
  process.stdout.write(name.padEnd(46));
  try {
    const value = await work();
    console.log("ok");
    return value;
  } catch (e) {
    console.log("FAIL");
    console.error("    " + (e instanceof Error ? e.message : String(e)));
    failed += 1;
    process.exit(1);
  }
}

const client = await step("connect to /rpc", () =>
  connect({ base, token, events: UI_EVENTS, autoSubscribe: false }),
);

const subscribed = await step("core.subscribe", () =>
  client.coreSubscribe({ events: UI_EVENTS, ext: { tally: true } }),
);
if (subscribed.ignored_ext.length) {
  console.log(`    this core ignored ext keys: ${subscribed.ignored_ext.join(", ")}`);
}

const state = await step("event/snapshot then event/flush", () => client.settled(10000));
console.log(`    flushed at seq ${state.seq}, ${state.sources.length} sources`);

const took = new Promise<string>((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error("no event/program.took in ten seconds")), 10000);
  const off = client.on("event", (e) => {
    if (e.name !== "program.took") return;
    const params = e.params as { source?: string | null };
    if (params.source !== ID) return;
    clearTimeout(timer);
    off();
    resolve(params.source);
  });
});

const added = await step("source.add test://smpte", () =>
  client.sourceAdd({ id: ID, uri: "test://smpte", name: "TypeScript end to end" }),
);

await step("program.take", () => client.programTake({ source: added.id }));
const onAir = await step("event/program.took", () => took);
console.log(`    programme is ${onAir}`);

await step("source.remove", () => client.sourceRemove({ id: added.id }));

client.close();
console.log("disconnect".padEnd(46) + "ok");
process.exit(failed ? 1 : 0);
