// The Stream Deck plugin: the only file that imports Elgato's SDK.
//
// Everything this file does is turn the SDK's callbacks into calls on the
// `Link` in `link.ts` and the functions in `keys.ts`. That is deliberate:
// `@elgato/streamdeck` cannot be fetched in this repository's test
// environment, so keeping the behaviour out of here is what lets `npm test`
// cover the plugin without it.
//
// Elgato's own contract, for a reader who has not written one before: a plugin
// is a Node process the Stream Deck app starts, it registers actions by UUID,
// and each action gets `onWillAppear`, `onKeyDown` and `onDidReceiveSettings`.
// Drawing is `setTitle` and `setImage`; there is no colour API, so a
// background colour is an SVG data URI, which `tile()` below builds.

import streamDeck, { action, SingletonAction } from "@elgato/streamdeck";
import type { KeyDownEvent, WillAppearEvent, DidReceiveSettingsEvent } from "@elgato/streamdeck";

import {
  outputLook,
  tile,
  pressOutput,
  pressSlate,
  pressTake,
  slateLook,
  takeLook,
  type KeyLook,
  type OutputSettings,
  type TakeSettings,
} from "./keys.ts";
import { Link, emptyView, type View } from "./link.ts";

/** The plugin's global settings: which mixer, and with what token. */
interface Global {
  base?: string;
  token?: string;
}

/** One connection for the whole plugin, shared by every key. */
class Mixer {
  private link: Link | null = null;
  view: View = emptyView();
  private listeners = new Set<(view: View) => void>();

  onChange(fn: (view: View) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  async connect(settings: Global): Promise<void> {
    this.link?.close();
    this.link = new Link({
      base: settings.base || "http://127.0.0.1:8080",
      token: settings.token || null,
      onChange: (view) => {
        this.view = view;
        for (const listener of this.listeners) listener(view);
      },
    });
    try {
      await this.link.open();
    } catch (error) {
      streamDeck.logger.warn(`cannot reach the mixer: ${String(error)}`);
    }
  }

  get handle(): Link | null {
    return this.link;
  }
}

const mixer = new Mixer();

/** What every action shares: draw on change, draw on appear. */
abstract class MixerAction<S> extends SingletonAction<S> {
  protected abstract look(view: View, settings: S): KeyLook;

  override async onWillAppear(event: WillAppearEvent<S>): Promise<void> {
    const draw = () => void this.draw(event, event.payload.settings);
    mixer.onChange(draw);
    draw();
  }

  override async onDidReceiveSettings(event: DidReceiveSettingsEvent<S>): Promise<void> {
    await this.draw(event, event.payload.settings);
  }

  protected async draw(event: { action: { setImage(url: string): Promise<void> } }, settings: S): Promise<void> {
    await event.action.setImage(tile(this.look(mixer.view, settings)));
  }
}

@action({ UUID: "com.godwinmix.streamdeck.take" })
export class TakeAction extends MixerAction<TakeSettings> {
  protected look(view: View, settings: TakeSettings): KeyLook {
    return takeLook(view, settings);
  }

  override async onKeyDown(event: KeyDownEvent<TakeSettings>): Promise<void> {
    if (!mixer.handle) return;
    streamDeck.logger.info(await pressTake(mixer.handle, mixer.view, event.payload.settings));
  }
}

@action({ UUID: "com.godwinmix.streamdeck.output" })
export class OutputAction extends MixerAction<OutputSettings> {
  protected look(view: View, settings: OutputSettings): KeyLook {
    return outputLook(view, settings);
  }

  override async onKeyDown(event: KeyDownEvent<OutputSettings>): Promise<void> {
    if (!mixer.handle) return;
    streamDeck.logger.info(await pressOutput(mixer.handle, mixer.view, event.payload.settings));
  }
}

@action({ UUID: "com.godwinmix.streamdeck.slate" })
export class SlateAction extends MixerAction<Record<string, never>> {
  protected look(view: View): KeyLook {
    return slateLook(view);
  }

  override async onKeyDown(): Promise<void> {
    if (!mixer.handle) return;
    streamDeck.logger.info(await pressSlate(mixer.handle));
  }
}

streamDeck.actions.registerAction(new TakeAction());
streamDeck.actions.registerAction(new OutputAction());
streamDeck.actions.registerAction(new SlateAction());

void streamDeck.settings.getGlobalSettings<Global>().then((settings) => mixer.connect(settings));
streamDeck.settings.onDidReceiveGlobalSettings<Global>((event) => void mixer.connect(event.settings));

streamDeck.connect();
