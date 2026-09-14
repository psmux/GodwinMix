// The one place this plugin names the GodwinMix client.
//
// In the repository it is imported by path, so `npm test` runs with nothing
// installed: no registry, no lockfile, no network. A published build changes
// this one line to
//
//     export * from "@godwinmix/client";
//
// and `package.json` already declares that dependency. Nothing else in this
// module imports the client, so that is the whole of the change.
export * from "../../../clients/typescript/src/index.ts";
