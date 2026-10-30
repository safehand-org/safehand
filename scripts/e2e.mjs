// End-to-end test of creating a version 2 card and signing it with three
// Nostr keys, in a real browser. Run locally, not in CI:
//
//   SPLITCARD_DEV=1 SPLITCARD_SKIP_ADDRESS_CHECK=1 \
//     PUBLIC_BASE_URL=http://localhost:3000/api BIND_ADDR=127.0.0.1:4001 ... splitcard-server
//   SERVER_INTERNAL_URL=127.0.0.1:4001 npm run dev -- -p 3000
//   node scripts/e2e.mjs
//
// APP_URL (default http://localhost:3000) is the app; CHROME_PATH is a
// Chrome or Chromium binary (default: Google Chrome on macOS).
//
// Alice and Bob have accounts; Alice adds Bob and Chidi by npub, Chidi
// never having used Safehand. Each signs in their own browser profile by
// pasting their nsec, and the card locks once all three have signed.

import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import puppeteer from "puppeteer-core";
import * as wasm from "../crates/splitcard-wasm/pkg/splitcard_wasm.js";

wasm.initSync({
  module: readFileSync(new URL("../crates/splitcard-wasm/pkg/splitcard_wasm_bg.wasm", import.meta.url)),
});

const APP = process.env.APP_URL ?? "http://localhost:3000";
const CHROME =
  process.env.CHROME_PATH ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const TIMEOUT = 30_000;

function person(name, address) {
  const secret = wasm.parse_secret_key(randomBytes(32).toString("hex"));
  const pubkey = wasm.public_key(secret);
  return { name, address, secret, pubkey, nsec: wasm.secret_key_nsec(secret), npub: wasm.npub_encode(pubkey) };
}

const alice = person("Alice", "alice@example.com");
const bob = person("Bob", "bob@example.com");
const chidi = person("Chidi", "chidi@example.com");
const team = `E2E ${Date.now().toString(36)}`;

function step(text) {
  console.log(`- ${text}`);
}

// A button whose text, or first span's text (before an icon), is `text`.
const button = (text) =>
  `::-p-xpath(//button[normalize-space()=${JSON.stringify(text)} or normalize-space(span[1])=${JSON.stringify(text)}])`;
const labelled = (text) =>
  `::-p-xpath(//label[.//span[normalize-space()=${JSON.stringify(text)}]]//input)`;
const textOnPage = (text) => `::-p-xpath(//*[contains(normalize-space(), ${JSON.stringify(text)})])`;

async function click(page, selector) {
  const element = await page.waitForSelector(selector, { visible: true, timeout: TIMEOUT });
  await element.click();
}

async function type(page, selector, text) {
  const element = await page.waitForSelector(selector, { visible: true, timeout: TIMEOUT });
  await element.click({ clickCount: 3 });
  await element.press("Backspace");
  await element.type(text);
}

async function see(page, text) {
  await page.waitForSelector(textOnPage(text), { timeout: TIMEOUT });
}

/** A fresh browser profile signed in by pasting the nsec, with a profile. */
async function signUp(browser, who) {
  const context = await browser.createBrowserContext();
  const page = await context.newPage();
  page.on("pageerror", (error) => console.error(`[${who.name}] page error:`, error.message));
  await page.goto(`${APP}/signup`, { waitUntil: "networkidle0" });
  await click(page, button("Sign In"));
  await click(page, button("Paste your nsec"));
  await see(page, "Only paste your key into apps you trust. A browser extension is safer.");
  await type(page, 'input[placeholder="nsec1…"]', who.nsec);
  const submits = await page.$$(button("Sign In"));
  await submits[submits.length - 1].click();
  await type(page, labelled("Display name"), who.name);
  await click(page, button("Sign Up"));
  await click(page, button("Skip"));
  await page.waitForFunction(() => window.location.pathname === "/", { timeout: TIMEOUT });
  return page;
}

/** Signs the open card from the sign prompt on the home screen. */
async function signFromPrompt(page, who) {
  await page.goto(`${APP}/`, { waitUntil: "networkidle0" });
  await see(page, `Alice added you to team ${team}`);
  await click(page, button("Sign"));
  await type(page, labelled("Your Lightning address"), who.address);
  await click(page, button("Sign with my Nostr key"));
}

async function api(path) {
  const response = await fetch(`${APP}/api${path}`);
  if (!response.ok) throw new Error(`GET ${path}: ${response.status} ${await response.text()}`);
  return response.json();
}

const browser = await puppeteer.launch({ executablePath: CHROME, headless: true });
try {
  step("Bob signs up");
  const bobPage = await signUp(browser, bob);

  step("Alice signs up and creates the card");
  const alicePage = await signUp(browser, alice);
  await alicePage.goto(`${APP}/create`, { waitUntil: "networkidle0" });
  await type(alicePage, labelled("Your name"), alice.name);
  await type(alicePage, labelled("Team name"), team);
  await type(alicePage, labelled("Which hackathon is this for?"), "Nostr Hack");
  await click(alicePage, button("Continue"));

  const npubField = 'input[aria-label="Teammate\'s npub"]';
  await see(alicePage, "Add everyone on your team by their npub.");
  await type(alicePage, npubField, alice.nsec);
  await alicePage.keyboard.press("Enter");
  await see(alicePage, "That's a secret key. Never share it.");
  const left = await alicePage.$eval(npubField, (input) => input.value);
  if (left !== "") throw new Error("the nsec was left in the field");

  await type(alicePage, npubField, bob.npub);
  await alicePage.keyboard.press("Enter");
  await alicePage.waitForSelector(`button[aria-label="Remove ${bob.name}"]`, { timeout: TIMEOUT });
  await type(alicePage, npubField, chidi.npub);
  await alicePage.keyboard.press("Enter");
  await see(alicePage, "What should we call them?");
  await type(alicePage, 'input[placeholder="Their name"]', chidi.name);
  await alicePage.keyboard.press("Enter");
  await alicePage.waitForSelector(`button[aria-label="Remove ${chidi.name}"]`, { timeout: TIMEOUT });
  await click(alicePage, button("Continue"));
  await see(alicePage, "33.3%");
  await click(alicePage, button("Continue"));
  await click(alicePage, button("Continue"));
  await type(alicePage, labelled("Your Lightning address"), alice.address);
  await click(alicePage, button("Sign with my Nostr key"));
  await alicePage.waitForFunction(() => window.location.pathname.startsWith("/card/"), {
    timeout: TIMEOUT,
  });
  await see(alicePage, "1 of 3 signed");
  const slug = decodeURIComponent(new URL(alicePage.url()).pathname.split("/")[2]);

  step("Bob signs from the prompt");
  await signFromPrompt(bobPage, bob);
  await see(bobPage, "2 of 3 signed");

  step("Chidi signs up for the first time and signs");
  const chidiPage = await signUp(browser, chidi);
  await signFromPrompt(chidiPage, chidi);

  step("The card is locked with everyone's signed address");
  let teamView = await api(`/teams/by-slug/${encodeURIComponent(slug)}`);
  for (let waited = 0; !teamView.active_card_hash && waited < TIMEOUT; waited += 500) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    teamView = await api(`/teams/by-slug/${encodeURIComponent(slug)}`);
  }
  if (!teamView.active_card_hash) {
    const alert = await chidiPage.$eval('[role="alert"]', (node) => node.textContent).catch(() => null);
    throw new Error(`the card did not lock${alert ? `: ${alert}` : ""}`);
  }
  const card = await api(`/cards/${teamView.active_card_hash}`);
  const addresses = card.members.map((member) => member.lightning_address);
  const expected = [alice.address, bob.address, chidi.address];
  if (JSON.stringify(addresses) !== JSON.stringify(expected)) {
    throw new Error(`payout addresses ${JSON.stringify(addresses)}`);
  }
  const verify = await api(`/cards/${teamView.active_card_hash}/verify`);
  const failed = verify.checks.filter((check) => check.result === "fail");
  if (failed.length > 0) throw new Error(`verify failed: ${JSON.stringify(failed)}`);
  console.log("e2e passed");
} finally {
  await browser.close();
}
