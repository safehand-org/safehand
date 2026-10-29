"use client";

import { DashedNote } from "@/components/join/JoinLayout";
import { homeDataFrom } from "@/lib/account";
import { useMe } from "@/lib/useMe";
import { HomeHeader } from "./HomeHeader";
import { HomeView } from "./HomeView";

/** Home for the signed-in member, from `GET /me`. */
export function HomeScreen() {
  const account = useMe();

  if (account.status === "ready") {
    return <HomeView data={homeDataFrom(account.me)} />;
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
