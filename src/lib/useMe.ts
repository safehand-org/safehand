"use client";

// Loads `GET /me` for a screen, sending the visitor to /signup when this
// browser has no login or the login has no profile yet.

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { ApiError, getMe, type Me } from "./api";
import { hasStoredKey } from "./identity";

export type MeState =
  | { status: "loading" }
  | { status: "ready"; me: Me }
  | { status: "error"; message: string; retry: () => void };

export function useMe(): MeState {
  const router = useRouter();
  const [state, setState] = useState<MeState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!hasStoredKey()) {
      router.replace("/signup");
      return;
    }
    let current = true;
    getMe()
      .then((me) => {
        if (!current) return;
        if (!me.profile) {
          router.replace("/signup");
          return;
        }
        setState({ status: "ready", me });
      })
      .catch((error: unknown) => {
        if (!current) return;
        if (error instanceof ApiError && error.code === "auth_missing") {
          router.replace("/signup");
          return;
        }
        setState({
          status: "error",
          message: error instanceof Error ? error.message : "Couldn't load your account.",
          retry: () => {
            setState({ status: "loading" });
            setAttempt((count) => count + 1);
          },
        });
      });
    return () => {
      current = false;
    };
  }, [router, attempt]);

  return state;
}
