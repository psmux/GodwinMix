// The Companion instance: the only file that imports Companion's module base.
//
// Everything this file does is hand Companion the four tables in
// `definitions.ts` and forward its callbacks into the `Link` in `link.ts`.
// That is deliberate. `@companion-module/base` cannot be fetched in this
// repository's test environment, so keeping the behaviour out of here is what
// lets `npm test` cover the module without it.
//
// Companion's own contract, for a reader who has not written a module before:
// a module is a Node process Companion starts, it extends `InstanceBase`, and
// `runEntrypoint` wires up the IPC. `init` is called with the operator's
// config, `configUpdated` when they change it, `destroy` on the way out.

import { InstanceBase, InstanceStatus, runEntrypoint } from "@companion-module/base";

import { ACTIONS, FEEDBACKS, VARIABLES, configFields, presets, variableValues } from "./definitions.ts";
import { Link, emptyView, type View } from "./link.ts";

interface Config {
  base: string;
  token: string;
}

class GodwinMixInstance extends InstanceBase<Config> {
  private link: Link | null = null;
  private view: View = emptyView();

  async init(config: Config): Promise<void> {
    this.setActionDefinitions(this.actions());
    this.setFeedbackDefinitions(this.feedbacks());
    this.setVariableDefinitions([...VARIABLES]);
    this.setPresetDefinitions(presets());
    await this.connect(config);
  }

  async configUpdated(config: Config): Promise<void> {
    await this.connect(config);
  }

  async destroy(): Promise<void> {
    this.link?.close();
    this.link = null;
  }

  getConfigFields(): unknown[] {
    return configFields();
  }

  private async connect(config: Config): Promise<void> {
    this.link?.close();
    this.updateStatus(InstanceStatus.Connecting);
    this.link = new Link({
      base: config.base || "http://127.0.0.1:8080",
      token: config.token || null,
      onChange: (view) => this.refresh(view),
      onStatus: (up, detail) =>
        this.updateStatus(up ? InstanceStatus.Ok : InstanceStatus.ConnectionFailure, detail),
    });
    try {
      await this.link.open();
    } catch (error) {
      this.updateStatus(InstanceStatus.ConnectionFailure, String(error));
    }
  }

  /**
   * One state change becomes one variable update and one feedback check.
   *
   * Both in the same tick, so a button's colour and its text never disagree,
   * and `checkFeedbacks` is called with the ids rather than bare so Companion
   * only re-renders the buttons that could have changed.
   */
  private refresh(view: View): void {
    this.view = view;
    this.setVariableValues(variableValues(view));
    this.checkFeedbacks(...(Object.keys(FEEDBACKS) as string[]));
  }

  private actions(): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const [id, action] of Object.entries(ACTIONS)) {
      out[id] = {
        name: action.name,
        options: action.options,
        callback: async (event: { options: Record<string, unknown> }) => {
          if (!this.link) return;
          try {
            await action.run(this.link, event.options);
          } catch (error) {
            // Every refusal from the core names the state and the next step.
            // Put that sentence in front of the operator rather than a stack.
            this.log("warn", `${id}: ${messageOf(error)}`);
          }
        },
      };
    }
    return out;
  }

  private feedbacks(): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const [id, feedback] of Object.entries(FEEDBACKS)) {
      out[id] = {
        type: feedback.type,
        name: feedback.name,
        description: feedback.description,
        defaultStyle: feedback.defaultStyle,
        options: feedback.options,
        callback: (event: { options: Record<string, unknown> }) =>
          feedback.check(this.view, event.options),
      };
    }
    return out;
  }
}

function messageOf(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

runEntrypoint(GodwinMixInstance, []);
