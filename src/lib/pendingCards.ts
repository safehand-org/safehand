// The drafts waiting for the viewer's signature, from `GET /me`. The sign
// prompt refreshes this list; the Cards tab badge and screens read it.

import { useSyncExternalStore } from "react";
import { getMe, type Me, type PendingCard } from "./api";
import { hasSigner } from "./signer";

const EMPTY: PendingCard[] = [];
let pending: PendingCard[] = EMPTY;
const listeners = new Set<() => void>();

function publish(next: PendingCard[]) {
  pending = next.length === 0 ? EMPTY : next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Reloads `GET /me`; resolves to it, or null without a login or a profile. */
export async function refreshPending(): Promise<Me | null> {
  if (!hasSigner()) {
    publish(EMPTY);
    return null;
  }
  const me = await getMe();
  publish(me.profile ? me.pending_cards : EMPTY);
  return me.profile ? me : null;
}

export function usePendingCards(): PendingCard[] {
  return useSyncExternalStore(subscribe, () => pending, () => EMPTY);
}
