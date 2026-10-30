"use client";

import { useEffect, useState } from "react";
import { DashedNote } from "@/components/join/JoinLayout";
import { homeDataFrom } from "@/lib/account";
import { currentNpub } from "@/lib/signer";
import { useMe } from "@/lib/useMe";
import { HomeHeader } from "./HomeHeader";
import { HomeView } from "./HomeView";

function useOwnNpub(serverPubkey: string | undefined) {
  const [loaded, setLoaded] = useState<{ pubkey: string; npub: string | null } | null>(null);

  useEffect(() => {
    if (!serverPubkey) return;
    let cancelled = false;
    void currentNpub(serverPubkey).then((value) => {
      if (!cancelled) setLoaded({ pubkey: serverPubkey, npub: value });
    });
    return () => {
      cancelled = true;
    };
  }, [serverPubkey]);

  if (!serverPubkey) return null;
  return loaded?.pubkey === serverPubkey ? loaded.npub : null;
}

/** Home for the signed-in member, from `GET /me`. */
export function HomeScreen() {
  const account = useMe();
  const serverPubkey = account.status === "ready" ? account.me.pubkey : undefined;
  const npub = useOwnNpub(serverPubkey);

  if (account.status === "ready") {
    return <HomeView data={homeDataFrom(account.me)} npub={npub} />;
  }

  // The sheet, without a name, while loading or after an error.
  return (
    <>
      <section className="relative overflow-hidden rounded-b-sheet bg-sheet px-5 pt-5 pb-6 text-ink">
        <HomeHeader />
        <div className="h-[8.5rem]" />
      </section>
      {account.status === "error" ? (
        <div className="px-5 pt-6">
          <DashedNote>
            {account.message}{" "}
            <button type="button" onClick={account.retry} className="text-[#34b6ee]">
              Try again
            </button>
          </DashedNote>
        </div>
      ) : null}
    </>
  );
}
