// The first picture goes on air by itself.
//
// A camera added to a mixer with nothing on air went into its scene and the
// programme stayed black, because nobody had put that scene on air, and a
// person reads that as "the camera does not work". The first thing added is
// what they want to see, so its scene goes on air, once, while nothing else
// is. Producer mode is left alone: there a take is the operator's alone.

import { toast } from "../../shell/toast.js";
import { settings } from "../../shell/settings.js";

export async function airIfNothingIs(client, scenes, scene) {
  const state = client.state || {};
  if (!scenes || !scene || settings().producer || state.program || state.scene) return false;
  try {
    await scenes.take(scene.id);
  } catch {
    return false; // the add worked; the Take button still does this by hand
  }
  toast({ text: `${scene.name} is on air.` });
  return true;
}
