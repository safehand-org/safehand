import type { ReactNode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { activityHomeData } from "./types";
import { HomeView } from "./HomeView";
import { shortNpub } from "./NpubShare";

vi.mock("next/image", () => ({
  default: function Image(props: { alt?: string; src?: string }) {
    const src = typeof props.src === "string" ? props.src : "";
    // Test stand-in for next/image; not shipped to the app.
    // eslint-disable-next-line @next/next/no-img-element
    return <img alt={props.alt ?? ""} src={src} />;
  },
}));

vi.mock("next/link", () => ({
  default: function Link({
    children,
    href,
  }: {
    children: ReactNode;
    href: string;
  }) {
    return <a href={href}>{children}</a>;
  },
}));

const NPUB = "npub10xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqpkge6d";

function renderHome(npub: string | null = NPUB) {
  return render(<HomeView data={activityHomeData} npub={npub} />);
}

describe("home npub", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows the short npub after the name", () => {
    renderHome();
    const name = screen.getByText("Flora.");
    const short = screen.getByText(shortNpub(NPUB));
    expect(name.compareDocumentPosition(short) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(shortNpub(NPUB)).toBe("npub10xlx…ge6d");
  });

  it("copies the full npub and shows Copied", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    renderHome();
    fireEvent.click(screen.getByRole("button", { name: NPUB }));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(NPUB);
      expect(screen.getByText("Copied")).toBeTruthy();
    });
  });

  it("selects the full npub when the clipboard is blocked", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    const selectNodeContents = vi.fn();
    const addRange = vi.fn();
    vi.spyOn(document, "createRange").mockReturnValue({
      selectNodeContents,
    } as unknown as Range);
    vi.spyOn(window, "getSelection").mockReturnValue({
      removeAllRanges: vi.fn(),
      addRange,
    } as unknown as Selection);
    renderHome();
    fireEvent.click(screen.getByRole("button", { name: "Copy your npub" }));
    await waitFor(() => {
      expect(screen.getByText(NPUB, { selector: "span.select-all" })).toBeTruthy();
    });
    expect(selectNodeContents).toHaveBeenCalled();
    expect(addRange).toHaveBeenCalled();
    expect(screen.queryByText("Copied")).toBeNull();
  });

  it("does not render an nsec on the home page", () => {
    const { container } = renderHome();
    expect(container.textContent).not.toMatch(/nsec/i);
    expect(container.innerHTML).not.toMatch(/nsec/i);
  });
});
