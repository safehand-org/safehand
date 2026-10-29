# Safehand backend integration PRD

Audience: one backend engineer wiring the existing Next.js frontend to an API in one pass. The frontend keeps its screens. The work is replacing mocks with fetches and mapping UI types onto server fields.

This document is the contract. It does not implement the API.

`crates/splitcard-server` is present and is the API to align with. It is an Axum service over SQLite (`splitcard-core` for card JSON, hashing, weights, and signatures). It already has NIP-98 auth, profiles, teams, join codes, card submit, signatures, OpenTimestamps, and verify routes. It does not have a signed-in user's card list, email invites, prize detection, payout rows, notifications, or history. Migration `0006_profiles_and_standalone_teams.sql` dropped the earlier `payouts` and `payout_items` tables. Prize and payout screens need a new read model, specified below.

Default server bind is `127.0.0.1:3000` (`BIND_ADDR`). NIP-98 `u` tags must equal `PUBLIC_BASE_URL` plus the path and query. Errors are always `{ "code", "message", "details" }`. Request bodies are at most 64 KB. A body that contains a `nostr+walletconnect:` or `nostrwalletconnect:` URI is rejected with `wallet_secret_rejected` (400). The browser pays Lightning itself. The server stores the result.

## Contents

1. [Product overview and routes](#1-product-overview-and-routes)
2. [Auth model](#2-auth-model)
3. [Domain entities](#3-domain-entities)
4. [Screen-by-screen contract](#4-screen-by-screen-contract)
5. [Card lifecycle and how pills map](#5-card-lifecycle-and-how-pills-map)
6. [Endpoints and JSON](#6-endpoints-and-json)
7. [Query params that go away](#7-query-params-that-go-away)
8. [Placeholder inventory](#8-placeholder-inventory)
9. [Frontend files to change](#9-frontend-files-to-change)
10. [Integration order](#10-integration-order)
11. [Out of scope](#11-out-of-scope)
12. [TODOs already in the code](#12-todos-already-in-the-code)

---

## 1. Product overview and routes

Safehand is a phone-width web app (`src/components/shell/PhoneShell.tsx`, max 390px) for hackathon teams to agree a prize split, sign it, lock it, and watch payouts. Product name in the layout metadata is Safehand (`src/app/layout.tsx`). The Rust crates call the same product Split Card.

There is no data fetching today. Every page renders a fixture or an empty client wizard. `src/app/card/[id]/page.tsx` is the only server component that reads an id, and it reads it from `lockedCards` in memory.

### Route map

| Route | File | Shell | What it shows today |
| --- | --- | --- | --- |
| `/` | `src/app/page.tsx` | nav, Home active | `HomeView` with `activityHomeData` |
| `/demo/home-activity` | `src/app/demo/home-activity/page.tsx` | nav, Home active | Same as `/`. Fixture route. |
| `/signup` | `src/app/signup/page.tsx` | no nav | Welcome, profile, three onboarding slides |
| `/cards` | `src/app/cards/page.tsx` | nav, Cards active | Summary, "needs signature", filterable list |
| `/create` | `src/app/create/page.tsx` | no nav | Six-step wizard, all local state |
| `/scan` | `src/app/scan/page.tsx` | no nav | Camera placeholder plus paste box |
| `/history` | `src/app/history/page.tsx` | nav, History active | Paid-out and cancelled sample |
| `/notifications` | `src/app/notifications/page.tsx` | nav, Notifications active | Three sample notifications |
| `/join` | `src/app/join/page.tsx` | no nav | Join wizard bound to `DEMO_INVITE` |
| `/invite/card/[code]` | `src/app/invite/card/[code]/page.tsx` | no nav | Same `JoinView`. The `code` param is ignored |
| `/card/[id]` | `src/app/card/[id]/page.tsx` | no nav | Locked-card status. Unknown id calls `notFound()` |

Bottom nav (`src/components/shell/BottomNav.tsx`): Home `/`, Cards `/cards`, Create `/create`, Scan `/scan`, History `/history`. Notifications are reached from the bell, which is a link on Home, Cards, Scan, History, and Notifications. The bell dot is painted on every header. It is not backed by an unread count. The hamburger button has no handler.

`/card/[id]` prebuilds static params from the keys of `lockedCards` (`generateStaticParams`). Real ids 404 until that function stops pinning the mock keys and the loader fetches.

Invite links the UI generates are `{origin}/invite/card/{slug}` where `slug` comes from `inviteSlug(teamName)` in `src/components/create/types.ts` (`Team 42` becomes `team-42`). The locked screen prints a separate display string, `yourapp/card/team-42`, in `LockedSplitCardDetail.cardLink`. That string is not a route. The real status route is `/card/team-42`. The real join route is `/invite/card/team-42`.

---

## 2. Auth model

Three entry paths. One identity underneath all of them.

### Identity the server already has

Every write sends `Authorization: Nostr <base64 NIP-98 event>`.

- Kind `27235`.
- `u` tag is `PUBLIC_BASE_URL` + path + query, exact.
- `method` tag is the HTTP method.
- `payload` tag is the SHA-256 of the raw body (empty body for GET when a header is sent).
- `created_at` within 60 seconds of the server clock.
- Event id is single-use (`auth_replayed` if repeated).

The caller's id is the event's pubkey (lowercase hex). There is no cookie, no OTP, and no email session. `GET /teams/{id}` and `GET /profiles/{pubkey}` accept a missing header (`OptionalAuth`). A present but bad header is still 401.

`PUT /me/profile` body is exactly `{ "name", "lightning_address" }` (`deny_unknown_fields`). `name` is non-empty. `lightning_address` is a lowercase LUD-16 address (`invalid_lightning_address` otherwise). Response is `ProfileView`: `pubkey`, `name`, `lightning_address`, `updated_at` (Unix seconds).

The frontend profile step also collects `bio` and the join step collects a display `color`. Those fields are not on `profiles`. Sending them to `PUT /me/profile` today returns 400 `invalid_request_json`. See [profile extensions](#profile-extensions).

### Path A — Signup (`/signup`)

Local state type `SignupState` in `src/components/signup/types.ts`. Persisted only in `sessionStorage` under `safehand.signup`, and deleted when onboarding finishes and the app navigates to `/`. That key is a wizard checkpoint, not a session.

| Step | UI | Mutation the screen already pretends to do |
| --- | --- | --- |
| `welcome` + `authMode: "create"` | "Create My Login", plus inert "Sign Up With Nostr" and "Sign Up With A Bitcoin Wallet" | Generate a local Nostr keypair and show a backup (nsec) before the profile step. Wallet signup uses a local signer. Do not POST an NWC URI. |
| `welcome` + `authMode: "signin"` | Dashed note: sign-in is not connected | Restore the local key or use NIP-07, then NIP-98. There is no second user table. |
| `profile` + `create` | Display name (required), bio (optional), "Sign Up" | `PUT /me/profile` with `name = displayName`. Lightning address is not on this screen. Collect it on the join profile step, or ask for it here before any team call. `profile_required` (409) blocks `POST /teams` and `POST /teams/{id}/join` until the profile exists. |
| `profile` + `signin` | Same dashed note, with a link back to create | Same as welcome sign-in. |
| `onboarding` slides 1–3 | Copy and stickmen. Skip or Let's Go goes to `/` | No API. |

`SignupView` renders a blank `min-h-full` div until `sessionStorage` has been read (`hydrated === false`). That is the only loading state on this route.

### Path B — Join (`/join` and `/invite/card/[code]`)

`JoinView` always starts from `DEMO_INVITE` and `createJoinState()` (step 1, first profile color, empty destination). It never reads the route param. Scan navigates to `/invite/card/${parseInviteCode(invite)}`. An empty paste becomes `team-42` because `parseInviteCode` falls back to `DEMO_INVITE.code`.

Production sequence for a person who opened an invite and has no key yet:

1. Land on `/invite/card/{slug}` with the join code in the query (`?join=`), because the server join code is 32 hex chars (`new_join_code` in `crates/splitcard-server/src/routes/teams.rs`) and the team slug is a separate LUD-16 username (3–32 chars, `a-z`, `0-9`, `-`, `_`, `.`, `+`, not in `RESERVED_SLUGS`).
2. If there is no local key, run Path A, then return to this URL.
3. `PUT /me/profile` if `name` or `lightning_address` is missing. The join profile step's destination field is that Lightning address. The mock fallback `greedy@lion.com` must not be submitted when the field is empty.
4. Resolve slug → team id (`GET /teams/by-slug/{slug}`, new).
5. `POST /teams/{id}/join` with `{ "join_code" }`. Idempotent. Wrong code is `invalid_join_code` (403).
6. Load the draft or active card and continue the join wizard.

The server does not bind an invite to an email. Anyone with the join code and a profile can join. The create-wizard sentence "Only the people you invited can sign. They'll sign in with the email you added." is copy. Emails typed in `StepTeammates` are not a server allow-list. One-shot keeps email as a label on the outgoing invite (the message the creator sends) and uses pubkey membership as the real allow-list.

Decline (`OutlineButton href="/"`) is navigation. There is no decline endpoint. An unsigned member simply never signs.

### Path C — Already identified

Use this when a local key exists and `GET /me` (new) shows the viewer is already on the team.

| Viewer | Join wizard start |
| --- | --- |
| No profile | Step 2 (profile), then join, then sign |
| Profile, not a participant, valid `join_code` | Step 1 (read the card) if they have not joined, then join on continue |
| Participant, pubkey in `signing.missing` | Step 3 (sign). Skip the demo profile |
| Participant, pubkey in `signing.signed` | Step 4 (you've signed) |
| Card `active` | Send them to `/card/{slug}` instead of the join wizard |

Home "Review And Sign" and Cards "Sign now" are buttons with no `href`. Point both at `/invite/card/{slug}?join={join_code}` for a card whose `signing.missing` contains the viewer.

### What the frontend must store

- Secret key, local only, after the backup step. Never send it to the server.
- Pubkey can be re-derived.
- Drop `safehand.signup` as an auth mechanism. Keep it only as the onboarding checkpoint until profile PUT succeeds.

There is no OTP field, OTP route, or verification-code component anywhere under `src/`. Email inputs are teammate addresses (`type="email"` on the create wizard) and the payout destination (`type="email"` on join, which is the Lightning address shape `local@domain`).

---

## 3. Domain entities

UI types are the shapes the screens render. Server types are what exist today. The adapter in each `types.ts` converts server JSON into the UI type. Percents on screen are derived from integer `weight`s. Amounts the server can actually store are sats. NGN on the mocks has no server source.

Timestamps in the database and in existing responses are Unix seconds. The screens render Africa/Lagos strings (`22 JAN, 2025`, `14:03 GMT +1`). Format in the adapter:

- Date: `D MMM, YYYY` uppercased month, e.g. `22 JAN, 2025`.
- Time: `HH:mm` 24-hour plus the literal ` GMT +1`.
- Notification age under 24 hours: `{n}H AGO` (home uses `2h ago`; notifications use `2H AGO`).
- History month group: full English month from the event time (`September`).
- History row date: `D MMM` uppercased (`30 SEP`).

### User

UI: `SignupState.displayName`, `SignupState.bio`, greeting `HomeGreeting.name` (includes a trailing period in the mock: `"Flora."`).

Server: `ProfileView` (`crates/splitcard-server/src/routes/profiles.rs`).

| UI | Server |
| --- | --- |
| `displayName` | `name` |
| greeting name | `name` + `"."` when the home title is `Welcome Back,` or `Hello,` |
| `bio` | not stored |
| avatar / member `color` | not stored |
| email | not an identity |

### SplitCard (list row)

UI type `SplitCard` in `src/components/cards/types.ts`:

```ts
{
  id: string;
  title: string;
  detail: string;
  status: "draft" | "locked" | "paid-out" | "cancelled";
  amount?: number;
  currency?: "sats" | "NGN";
  phase?: "waiting-for-prize" | "prize-set-waiting" | "prize-in-card" | "payouts-in-progress";
  href?: string;
}
```

Server card row status is `draft | active | superseded`, plus computed `stale` when a draft's `supersedes` no longer matches `teams.active_card_hash`. That is not the list pill. Pill mapping is in [section 5](#5-card-lifecycle-and-how-pills-map).

`detail` is a sentence the adapter builds:

| Situation | `detail` |
| --- | --- |
| Draft card, some signatures | `"{signed} of {members} signed"` |
| Active, no prize | `"Everyone has signed. Waiting for the prize"` |
| Payout in progress, viewer has been paid | `"{amount} sats paid to you"` with `amount` and `currency: "sats"` |
| Cancelled | `"A teammate left"` (history uses this; the cards list has no cancelled sample) |

`id` on the row is the team slug, because `/card/[id]` and the mocks use `lagos-build` and `team-42`. The card hash stays on the detail payload for signature POST. `href` is `/card/{slug}` when the pill is `locked` or `paid-out`. Draft rows have no `href` (`CardList` renders a non-link). `cancelled` rows have no detail route.

### Member

Three UI shapes for the same person:

| Screen | Type | File |
| --- | --- | --- |
| Locked card | `LockedMember` | `src/components/cards/locked/types.ts` |
| Join | `JoinMember` | `src/components/join/types.ts` |
| Create wizard | `Member` / `Teammate` | `src/components/create/types.ts` |

Server: `Participant` on a team (`pubkey`, `name`, `lightning_address` only for co-members, `joined_at`) and `splitcard_core::card::Member` on a card (`pubkey`, `name`, `weight`, `payout.lightning.address`).

Map:

| UI field | Source |
| --- | --- |
| `id` | `pubkey` |
| `name` / `shortName` | card `name` (shortName is the same string; the mock shortens only Spongebob → `"Spongebob"`) |
| `sharePercent` / `percent` | `weight / sum(weights) * 100`, one decimal the way `formatPercent` does (`toFixed(1)`) |
| `color` | local palette until profile color exists. Do not invent a color from the API |
| `signed` | pubkey is in `CardView.signing.signed` |
| `you` | pubkey equals the viewer |
| `payoutStatus` | payout item status, only on phase `payouts-in-progress` |
| `payoutSats` | that member's sats. The field is on `LockedMember` and is not rendered. The row shows the percent and a Paid/Pending chip |
| `email` | create-wizard only, not stored |

`WhoGetsWhat` in `LockedCardStatusView.tsx` reads `members[0]`, `members[1]`, `members[2]` and returns null when any is missing. The agreed-share list renders every member. Return the full member array in card order. Teams of two still render the list. The three-slot bar stays blank until a later UI change. That bar change is out of scope for this pass.

Equal three-way split in the mocks is `33.3` three times (99.9). The wizard accepts a total in `[99.8, 100.2]`. On submit, convert percents to weights that sum exactly, for example basis points `Math.round(percent * 10)` adjusted so the weights sum to 1000. `weight` must be `>= 1` and `<= 2^53-1`. `compute_payouts` rejects a share that rounds to 0 sats.

### Share

Create: `ShareMode = "equal" | "custom"`, `customShares: Record<id, number>`, `leavePolicy: "nothing" | "smaller"`, `leaveKeepPercent` default `15`.

Server card `rules` is only `{ "peer_review": null | { "max_shift_points": number } }` and `deny_unknown_fields`. Leave policy cannot be stuffed into the card JSON. The signed agreement copy on join and on the locked screen is built in the client from percent, destination, and `leaveKeepPercent`. One-shot: add optional team columns (below) or keep the leave sentence as client copy with the default 15. Do not fail card submit because of it.

Payout destination on the card is `members[].payout = { "lightning": { "address": "<lowercase LUD-16>" } }`. The locked screen's `shareDestination` is the viewer's own address on the card, not a single team address. The mock uses one address for every agreement (`greedy@lion.com`).

### Invite

UI `DEMO_INVITE`:

```ts
{
  code: "team-42",          // used as slug AND as the path code
  email: "goodness@friedrice.com",
  inviter: "Flora",
  teamName: "Team 42",
  hackathon: "Lagos Builders Hackathon",
  leaveKeepPercent: 15,
  sharePercent: 33.3,
  members: JoinMember[]     // Goodness has you: true, signed: false
}
```

Server invite is `teams.join_code` (32 hex, revealed only to participants on `GET /teams/{id}`) plus `teams.slug`. Public invite resolution is a new route because `GET /teams/{id}` requires a numeric id (`team_not_found` for a slug).

Suggested public invite payload (no Lightning addresses, no join code unless the query presents it and it matches):

```json
{
  "team_id": "42",
  "slug": "team-42",
  "name": "Team 42",
  "event_label": "Lagos Builders Hackathon",
  "created_by_name": "Flora",
  "join_code_ok": true,
  "viewer": "missing_signature",
  "leave_policy": "smaller",
  "leave_keep_percent": 15,
  "members": [
    { "pubkey": "…", "name": "Flora", "weight": 1, "signed": true }
  ]
}
```

`viewer` is one of `anonymous | needs_profile | needs_join | missing_signature | signed | locked`. `leave_*` is omitted when the column does not exist yet; the UI then keeps its current 15% sentence.

### Signature

Server: `POST /cards/{hash}/signatures` with the raw Nostr event JSON as the body. The event is the auth (this route does not use the NIP-98 extractor). 201 when stored, 200 when that pubkey already signed. Response is `CardView`.

The join step "Sign Card" must build that event over the card hash with the local key, then POST it. The screen currently only increments `JoinState.step`.

`CardView.signatures` is the stored event JSON strings. `signing` is:

```json
{
  "state": "draft",
  "signed": ["<pubkey>", "<pubkey>"],
  "missing": ["<pubkey>"],
  "rejected": []
}
```

`signing.state` is `draft` until every card member has signed, then `active`. Stored `cards.status` becomes `active` in the same moment, `activated_at` is set, the previous card (if `supersedes`) becomes `superseded`, and `teams.active_card_hash` updates.

### Payout

UI `LockedPrize`, `payoutsSummary: { paid, total }`, `LockedMember.payoutStatus: "paid" | "pending"`, list `amount` + `currency`.

No payout table remains after migration 0006. `splitcard-core::split::compute_payouts(card, prize_sats)` returns `(pubkey, sats)` in member order and is the split math to use when a prize exists. The server never sends the payment.

New read model the screens need, one row per team (the dropped schema was also one payout per team):

```json
{
  "team_id": "42",
  "card_hash": "<hex sha256>",
  "prize_sats": 410000,
  "currency_display": "sats",
  "phase": "payouts-in-progress",
  "prize_known_at": 1737550980,
  "prize_received_at": 1737550980,
  "items": [
    {
      "pubkey": "…",
      "name": "Goodness",
      "lightning_address": "goodness@example.com",
      "amount_sats": 164000,
      "status": "paid",
      "paid_at": 1737551000
    }
  ]
}
```

`status` on an item for the UI chip: `paid` → chip Paid, anything else (`pending`, `invoiced`, `paying`, `failed`, `unknown`) → chip Pending. The progress label is `"{count of paid} of {items.length} paid"`.

`currency_display` is `sats` for anything the server computed. The mocks also show `NGN` and the badge words `RECEIVED | ROUND | IN CARD | PAYOUT`. Badge map:

| Phase | Badge | Prize card visible |
| --- | --- | --- |
| `waiting-for-prize` | none | no |
| `prize-set-waiting` | none (mock sets `amount` 410000 NGN and still hides the prize card) | no |
| `prize-in-card` | `IN CARD` | yes |
| `payouts-in-progress` | `PAYOUT` | yes |

The badges `RECEIVED` and `ROUND` are on the type `LockedPrize.badge` and are not used by any fixture.

### Notification

UI `AppNotification` in `src/components/notifications/types.ts`: `id`, `title`, `body`, `time`, `kind: "card" | "payout"`, `featured?`, `actionLabel?`.

No table. Minimum one-shot feed, newest first, derived plus one stored kind for "you were invited" once invite delivery exists:

| Event | `kind` | `title` / `body` the way the sample is written | `featured` |
| --- | --- | --- | --- |
| Viewer in `signing.missing` | `card` | `"{inviter} invited you to {team}"` / `"Your share is {percent}%. Read the card before you sign."` | the newest one, with `actionLabel: "Review And Sign"` |
| Card `activated_at` set | `card` | `"{team} is locked"` / `"Everyone has signed. Waiting for the prize."` | no |
| Viewer's payout item `paid` | `payout` | `"You received {amount} sats"` / `"{team name}"` | no |

`time` is the formatted timestamp. `featured` is a client choice: the newest unread card-kind item. The sample hardcodes one featured row in the sheet and the rest under "Earlier".

### Activity

UI `HomeActivityItem`: `id` (slug), `title`, `detail`, `status` (`"Draft" | "Locked" | "Paid out"` as rendered text), optional `href`.

Same rows as the cards list, shorter. Home shows a handful, not the full filterable list. `status` is the pill label (`Draft`, `Locked`, `Paid out`). `ActivityPill` also accepts `"paid-out"`. `href` is set for Locked and Paid out (`/card/{slug}`) and omitted for Draft, matching `CardList`.

`HomeAttention` is the single "needs your signature" card on home: `title` (team name), `body` (`"{inviter} invited you to a split. Your share is {percent}%."`), `timeAgo`, `teammatesLabel` (`"{n} teammates"`), `actionLabel` (`"Review And Sign"`). Null when the viewer is in nobody's `signing.missing`.

### History

UI `HistoryCard`: `id`, `title`, `detail`, `amount` (preformatted `"250,000 SATS"` or null), `date` (`"30 SEP"` or null), `month`, `status: "paid-out" | "cancelled"`.

Include a team when its payout phase is `fully-paid-out` (all items paid) or its product status is `cancelled`. Grouping by `month` is client-side (`groupHistoryByMonth`). Rows are not links.

`detail`: `"{n} people"` or `"{n} teammates"` for paid-out, `"A teammate left"` for cancelled. The sample uses both "people" and "teammates". Pick `"{n} teammates"` and keep the cancelled sentence.

---

## 4. Screen-by-screen contract

Global: there is no shared React Query, no error banner, and no authenticated fetch helper. Each view either receives props from a server page or imports a fixture. Loading and error UI below is what to do inside the existing components when the fixture becomes a fetch. Empty states that already exist should be reused.

Filters (cards, history, notifications) stay in `useState` and run on the array already loaded. Refetch when the user opens the page or pulls a mutation that changes membership, signatures, or payouts. Do not refetch on chip click.

### Home — `/`

**Mock today.** `activityHomeData` from `src/components/home/types.ts`, passed by `src/app/page.tsx` and by `/demo/home-activity`.

- Greeting: title `Welcome Back,`, name `Flora.`, subtitle `You have 1 card waiting for your signature.`
- Attention: title `Orbit Pay`, body `Goodness invited you to a split. Your share is 33%.`, `2h ago`, `3 teammates`, button `Review And Sign` (no navigation).
- Invite banner: `Join in with the link or scan the qr code.`
- Activity: `Team 42` / `1 of 3 signed` / `Draft` (no href); `Lagos Build` / `Everyone has signed. Waiting for the prize` / `Locked` / href `/card/lagos-build`.

`emptyHomeData` is a second fixture in the same file. It is not used by `/`. `SignupOnboarding` blurs it behind the slides.

- Greeting: `Hello,` / `Flora.` / `No split cards yet. Let's make your first one in two minutes.`
- `attention: null`
- Invite: `Got an invite? Open the link or scan the code to join your team.`
- `activity: []`, which renders `ActivityEmpty` ("No activity record yet.") with links to `/create` and `/scan`.

**Load.** `GET /me` plus the team summaries in that payload.

| Field | Rule |
| --- | --- |
| `greeting.title` | `Welcome Back,` when `teams.length > 0`, else `Hello,` |
| `greeting.name` | `profile.name + "."` |
| `greeting.subtitle` | `You have {n} card(s) waiting for your signature.` when `n > 0`, else the empty subtitle when there are no teams, else a neutral line the existing type already allows (one string) |
| `attention` | Newest team where viewer is in `signing.missing`, or null |
| `inviteMessage` | The two strings already in the fixtures. Choose the empty one when `teams.length === 0` |
| `activity` | Up to the same rows as cards, status labels `Draft`, `Locked`, `Paid out` |

**Mutations.** None. Attention button should link to the invite URL. That is a one-line `href`, same as the cards "Sign now" button.

**States.**

| State | UI |
| --- | --- |
| Loading | The page is a server component today. Fetch in the page and pass `HomeData`, or show the sheet shell the card page already uses. Do not flash `Flora.` |
| Empty | `activity: []` and `attention: null` hit `ActivityEmpty`. Use `emptyHomeData` shape with the real name |
| Error | Keep the shell. A failed `GET /me` with `auth_missing` goes to `/signup` |
| Success | `HomeView` as it is |

**Local vs refetch.** Nothing is filtered on home. Refetch when the page loads. The bell dot stays painted; do not invent a count on this pass unless `GET /me` includes `unread_notifications`.

### Cards — `/cards`

**Mock today.** `cardsSample` in `src/components/cards/types.ts`. Header summary and signature card are hardcoded in the components, not in the sample.

`SummaryCard` (`src/components/cards/SummaryCard.tsx`): `410,000` `sats`, `2 Split Paid`, `1 Cancelled`. Stickman is decoration.

`SignatureCard` (`src/components/cards/SignatureCard.tsx`): title `Bikini Bottom`, body `Goodness invited you. Your share is 45%.`, button `Sign now` with no handler.

`cardsSample`:

| id | title | detail | status | extra |
| --- | --- | --- | --- | --- |
| `kano-hack-night` | Kano Hack Night | `1 of 3 signed` | `draft` | no link |
| `lagos-build` | Lagos Build | `Everyone has signed. Waiting for the prize` | `locked` | links to `/card/lagos-build` |
| `team-42` | Team 42 | `250,000 sats paid to you` | `paid-out` | `amount: 250000`, `currency: "sats"`, `phase: "payouts-in-progress"`, `href: "/card/team-42"` |

Filters `cardFilters`: `all`, `locked`, `paid-out`, `cancelled`, `drafts`. `cardsForFilter` is pure. `drafts` compares `status === "draft"`. Empty list copy is always `"No cancelled cards."` even when the active chip is Drafts or Locked (`CardList`). Leave that string. It is wrong for other chips and fixing the sentence is a copy change, listed as out of scope.

**Load.** Same `GET /me` team summaries. Summary numbers:

- Total received: sum of the viewer's `paid` payout items, in sats. Label stays `sats`. The mock number `410,000` is the prize figure reused as a personal total. Do not copy it.
- `{n} Split Paid`: count of teams in `fully-paid-out` or with at least one paid item for the viewer. The mock says 2.
- `{n} Cancelled`: count of `cancelled` teams. The mock says 1. The sample list contains zero cancelled rows, so the summary and the list disagree on purpose in the fixture. Use the real count.

Signature strip: same object as home `attention`, hidden when null. `Sign now` links to the invite or sign URL.

**Mutations.** None on this page.

**States.** Loading: header can render, list area empty until the payload arrives. Empty filter: the existing sentence. Error: the list area shows nothing new (no error component exists); a 401 sends the user to `/signup`. Success: `CardsView` with the array in state.

**Local vs refetch.** Chip clicks filter the loaded array. Refetch on mount and after returning from create, join, or sign.

### Card status — `/card/[id]`

**Mock today.** `getLockedCard(id, phase)` in `src/components/cards/locked/types.ts`. `phase` comes from `?phase=`. Unknown id → `notFound()`. `lagos-build` ignores `phase` and always returns the waiting fixture. Any other known id rebuilds a Team 42 fixture for the resolved phase.

Known ids and the fixture they return when `phase` is absent:

| id | Fixture | phase |
| --- | --- | --- |
| `lagos-build` | Lagos Build, Flora created it, lock `12 AUG, 2026` `14:03 GMT +1`, link `yourapp/card/lagos-build` | `waiting-for-prize` |
| `team-42` | Team 42 payouts | `payouts-in-progress` |
| `team-42-waiting` | Team 42 waiting | `waiting-for-prize` |
| `team-42-payout` | Team 42 payouts | `payouts-in-progress` |
| `team-42-paid` | prize in card, members Favour / Goodness / Solomonfaith | `prize-in-card` |
| `team-42-received` | same as paid | `prize-in-card` |
| `team-42-payouts` | Team 42 payouts | `payouts-in-progress` |

Shared waiting/payout members: Flora or Flare (`#43c7ff`), Goodness (`#f78125`), Spongebob Squarepants (`#8a38f5`), each signed. Prize-in-card swaps in Favour 30%, Goodness 40%, Solomonfaith 30%. Payouts use Flare 30% paid, Goodness 40% paid, Spongebob 30% pending, `payoutsSummary: { paid: 2, total: 3 }`. Prize amount constant `PRIZE_AMOUNT = 410_000`. Destination `greedy@lion.com`. Team 42 lock `22 JAN, 2025` `14:03 GMT +1`. Card link `yourapp/card/team-42`. Slug field is `team-42` even when the route id is `team-42-paid`.

Agreements (three cards, check badges are decorative):

1. `Your share is {percent}` / `Paid to {destination} when the prize arrives.`
2. `If you leave before you submit, you keep 15% of the prize.` / `The rest is split equally among the people who stay.`
3. `Changes need everyone to agree.` / `Once the card is locked, no one can change it alone.`

`prize-in-card` sets `agreements: []` and `timeline: []`, so those sections hide.

Timeline labels are sentences (`Flora created split card 'Team 42'`, `Goodness and Spongebob signed card`, `Card locked`, closing `Prize paid out` or `Final split out` with `pending: true`). Build them from `created_by`, signature order, `activated_at`, and payout phase. Pending events use the label and `pending: true` (the UI prints `Pending` in accent color instead of a date).

**Load.** `GET /teams/by-slug/{id}` (the route id is the slug), then `GET /teams/{team_id}/card` or the latest draft, then `GET /teams/{team_id}/payout` when that route exists. Assemble `LockedSplitCardDetail`.

| UI field | Source |
| --- | --- |
| `id`, `slug` | team slug |
| `title` | `team.name` |
| `phase`, `payoutPhase` | derived, [section 5](#5-card-lifecycle-and-how-pills-map). Drop the query string |
| `headerVariant` | derived from phase via `headerVariantFor` |
| `subtitle` | `WAITING_SUBTITLE` when phase is `waiting-for-prize` or `prize-set-waiting`. `SIGNED_SUBTITLE` otherwise, with the team name interpolated (`All teammates for '{title}' have signed the split card.`) |
| `cardLink` | `{host}/card/{slug}` with host from `PUBLIC_BASE_URL` or the site origin. Share prepends `https://` when the string has no scheme |
| `lockedOn` | `activated_at` formatted |
| `members` | card members + signing + payout items |
| `agreements` | built in the adapter from the viewer's weight, the viewer's lightning address, and leave policy. Empty array on `prize-in-card` to match the layout switch already in `sectionCopy` |
| `timeline` | built in the adapter. Empty array on `prize-in-card` |
| `prize` | set on `prize-in-card` and `payouts-in-progress` only |
| `amount`, `currency` | `prize_sats`, `sats` |
| `payoutsSummary` | set only when phase is `payouts-in-progress` |
| `shareDestination` | viewer's `payout.lightning.address` |

`sectionCopy` already switches headings. Do not send headings from the API.

**Mutations.** Share button copies or shares `cardLink` (`StatusFooter`). Go Home links to `/`. No other writes. Signing happens on the join flow, not here.

**States.**

| State | UI |
| --- | --- |
| Loading | `LockedStatusFallback`: a `h-72` rounded sheet. Already wired through `Suspense` |
| Empty | This page has no empty state. A team with no card is `notFound()` or a redirect to `/cards` |
| Error | `card_not_found`, `team_not_found`, `no_active_card` for a slug that is not a locked card: `notFound()`. Network failure: the fallback stays; do not render Flora's Team 42 |
| Success | `LockedCardStatusView` |

**Local vs refetch.** No filters. Refetch on mount. Ignore `searchParams.phase`.

### Create — `/create`

**Mock today.** `WizardState` from `createInitialState()` in `src/components/create/types.ts`. Nothing is saved. "Save As Draft" and "Save Draft" are links to `/cards`. "Send Invites" advances to step 6. "Go To My Cards" links to `/cards`.

| Step | Title | Fields | Gate |
| --- | --- | --- | --- |
| 1 | Create Split Card, or Create A Team Split Card once the three fields are non-empty | `yourName`, `teamName`, `hackathon` | all trimmed non-empty |
| 2 | Who Are You Building With? | creator row (name, or `"Flora"` if the name were empty), teammate email | at least one teammate |
| 3 | How Should The Prize Be Shared? | `shareMode` `equal` or `custom` | total in `[99.8, 100.2]` |
| 4 | What If Someone Leaves? | `leavePolicy` `nothing` (default) or `smaller`, `leaveKeepPercent` default 15 | none. Secondary button leaves |
| 5 | Review Your Split Card | team, hackathon, members with percents | none |
| 6 | Invite Your Team | QR of `{origin}/invite/card/{slug}`, path text, share | none |

Step 2 email helpers: `isValidEmail`, duplicate emails ignored, `nameFromEmail`. Known map:

- `goodness@friedrice.com` → `Goodness`
- `spongebobeddie@pineapple.com` → `Spongebob Squarepants`
- anything else → title-cased local part, or `"Teammate"`

Colors cycle `AVATAR_PALETTE` (`#8B2C2C`, `#5C1518`, `#1F4D3A`, `#1E3A5F`, `#6B3FA0`). Creator id is the constant `"creator"`. Teammate id is `teammate-{email}`. Those ids are local. Replace them with pubkeys once people have joined. Until a teammate has joined, the card cannot list them: `POST /teams/{id}/cards` rejects non-participants (`member_not_participant`).

`inviteSlug` lowercases and strips non-alphanumerics to hyphens. The server slug is stricter (length 3–32, charset, `RESERVED_SLUGS`, unique). Generate the server slug with the same function, then `check_slug`. On `team_slug_taken` (409) or `invalid_slug` (422), keep the user on step 1 with the team name still filled. There is no error text in the wizard today; a disabled Continue is the only failure style. Surface the `message` from the error JSON on the step that failed. That is the error state.

**Mutations, in order, at "Send Invites" (step 5 → 6), not on every Continue.**

1. Ensure profile: `PUT /me/profile` `{ "name": yourName, "lightning_address": <creator address> }`. The wizard never asks for the creator's Lightning address. Block send until it exists on the profile (collect it on step 1 or refuse with the profile error). Do not invent `greedy@lion.com`.
2. `POST /teams` `{ "slug", "name": teamName, "event_label": hackathon }`. Response includes `id`, `join_code`, `participants` (the creator).
3. If leave columns exist: `PUT /teams/{id}` body `{ "leave_policy", "leave_keep_percent" }`. This route does not exist. Skip it and keep the sentence local when it 404s.
4. Do not submit the card until every invited person is a participant, because the card member list must be participants and must match their current profile name and Lightning address. Step 6 stays "waiting for N teammates". When `GET /teams/{id}` shows everyone joined, `POST /teams/{id}/cards` with the canonical card, then `POST /cards/{hash}/signatures` for the creator.

"Save As Draft" (step 4) and "Save Draft" (step 5): perform steps 1–2 only, then route to `/cards`. The list row is `draft` with detail `0 of {n} signed` while the card is not submitted, or `{signed} of {n} signed` after submit. The wizard does not reload a saved team. Re-opening a draft is out of scope; the row stays a non-link, which is what `CardList` does now.

Step 6 QR and path: `{origin}/invite/card/{slug}?join={join_code}`. `parseInviteCode` already extracts a path code. It does not read `join`. Extend the parser to keep the query when wiring scan and join. The QR is drawn in the browser by `QrCode` (`src/components/create/qr.tsx`). No QR endpoint.

**States.** Loading: disable the primary button while a request is in flight (the button already disables on incomplete input). Empty: step 2 copy "Add at least one teammate to continue." Error: show `message` on the active step. Success: step 6 with the real URL.

**Local vs refetch.** The whole wizard is local until Save or Send. After send, refetch is the cards page's job.

Canonical card the POST body must be (this is the raw JSON `POST /teams/{id}/cards` already accepts; the response wraps it as a string on `CardView.card`):

```json
{
  "version": 1,
  "card_id": "team-42-1",
  "event": "Lagos Builders Hackathon",
  "team": "Team 42",
  "members": [
    {
      "pubkey": "<creator pubkey>",
      "name": "Flora",
      "weight": 1,
      "payout": { "lightning": { "address": "flora@example.com" } }
    }
  ],
  "rules": { "peer_review": null },
  "supersedes": null
}
```

`event` must equal `teams.event_label` (`wrong_event`). `team` must equal `teams.name` (`wrong_team`). `name` and lightning address must equal the profile at submit time. `supersedes` is null for the first card and the active hash for a replacement (`unexpected_supersedes`, `missing_supersedes`, `wrong_supersedes_hash`). The UI has no "change the locked card" screen. Do not build supersede in this pass.

`card_id` is a stable string the core schema requires. Use `{slug}-{n}`. It is not the route id.

### Join — `/join` and `/invite/card/[code]`

**Mock today.** `DEMO_INVITE` and `DEMO_SHARE_DESTINATION` (`greedy@lion.com`).

| Step | What it shows | Writes |
| --- | --- | --- |
| 1 | `{inviter} Invited You To Sign A Split Card`, team name, hackathon, member percents, leave note with `leaveKeepPercent` | Continue is local. Decline goes `/` |
| 2 | Display color (`PROFILE_COLORS`), "Where should your share go?" | Local `avatarColor`, `shareDestination` |
| 3 | Three agreement cards. Destination is the field or `DEMO_SHARE_DESTINATION` | "Sign Card" is local |
| 4 | Members with Signed / Not yet signed. The `you` row flips to signed and uses `avatarColor` | "Go Home" |

`StepSigned` still reads `DEMO_INVITE.members`. After the POST, re-read `signing` and render that.

**Load.** On mount, `GET /invites/{slug}?join=` (new) or `GET /teams/by-slug/{slug}`. Drive the initial step from `viewer` in [Path C](#path-c--already-identified).

**Mutations.**

- Step 2 Continue: `PUT /me/profile` `{ "name": <already on profile or the invitee's name>, "lightning_address": shareDestination }`. Color stays in component state. If `color` is added to the profile, send it here. The join screen does not edit the display name; the name on the card is the profile name, which must already match when the card was submitted. If this person is not on the card yet, joining the team is what lets the creator include them. Order in the UI is "read card → profile → sign", which assumes the card already lists them. That matches a card submitted after they joined. If they open the link before the card exists, step 3 waits on `no_active_card` / no draft and step 4 says the signature is saved only after a real 201.
- Step 3 Sign Card: `POST /teams/{id}/join` if not yet a participant, then `POST /cards/{hash}/signatures`.

**States.** Loading: blank column until the invite loads (same idea as signup's `hydrated` blank). Empty code: do not substitute `team-42`. Show the scan field's error by staying on `/scan` is wrong here; on this route an unknown slug is `notFound()`. Error: `invalid_join_code` stays on step 1 with the server `message`. Success: step 4 from `signing.signed`.

**Local vs refetch.** Step index is local. Members, percents, inviter, and signed flags come from the payload and refresh after sign.

### Scan — `/scan`

**Mock today.** Camera is an icon in a square. No `getUserMedia`. Paste box placeholder `Paste the link or type the code.` Submit and "Open card" both call `parseInviteCode` and `router.push(/invite/card/${code})`. Empty input navigates to `/invite/card/team-42`.

**Load.** None.

**Mutations.** None. Navigation only.

**States.** Loading: none. Empty input: stay on the page. Do not fall back to `DEMO_INVITE.code`. Error: a code that 404s is the invite page's `notFound()`. Success: `/invite/card/{slug}?join={code}` when the paste contains both, else `/invite/card/{slug}`.

`parseInviteCode` behavior to keep: trim, pull `invite/card/{segment}` out of a full URL, `decodeURIComponent`, strip leading and trailing slashes. Behavior to delete: the `DEMO_INVITE.code` fallback.

The camera control can stay a placeholder. Decoding a QR image is out of scope. Pasting the link is the integrated path.

### History — `/history`

**Mock today.** `historySample` in `src/components/history/types.ts`.

| id | title | detail | amount | date | month | status |
| --- | --- | --- | --- | --- | --- | --- |
| `nairobi-sprint` | Nairobi Sprint | `3 people` | `250,000 SATS` | `30 SEP` | September | `paid-out` |
| `abuja-hack` | Abuja Hack | `2 teammates` | `40,000 SATS` | `21 SEP` | September | `paid-out` |
| `accra-weekend` | Accra Weekend | `A teammate left` | null | null | August | `cancelled` |

Filters: `all`, `paid-out`, `cancelled`, applied by `historyForFilter`. Empty copy: `No cards in this filter.`

**Load.** `GET /me/history` (new) or filter `GET /me` teams to `fully-paid-out` and `cancelled`. Amount string: `"{n.toLocaleString("en-US")} SATS"`. Date from the payout `paid_at` (latest item) or the cancel time. Cancelled rows pass null amount and null date so `HistoryList` renders only the right-aligned status.

**Mutations.** None.

**States.** Loading: header and title render, list waits. Empty: the existing sentence (also the real empty state when no payout has completed). Error: 401 → `/signup`. Success: grouped months.

**Local vs refetch.** Chips filter locally. Refetch on mount.

### Notifications — `/notifications`

**Mock today.** `notificationsSample` in `src/components/notifications/types.ts`.

| id | title | body | time | kind | extra |
| --- | --- | --- | --- | --- | --- |
| `bikini-invite` | Goodness invited you to Bikini Bottom | Your share is 45%. Read the card before you sign. | `2H AGO` | `card` | `featured`, action `Review And Sign` linking to `/cards` |
| `lagos-locked` | Lagos Build is locked | Everyone has signed. Waiting for the prize. | `2 OCT` | `card` | |
| `nairobi-payout` | You received 250,000 sats | Nairobi Sprint | `30 SEP` | `payout` | |

Filters: `all`, `card` (label Cards), `payout` (label Paid Out). Featured row is the first `featured` item in the filtered list and is pulled out of "Earlier". Empty earlier list: `No notifications in this filter.` "Mark all as read" has no handler. The action link is hardcoded to `/cards`, not to the card.

**Load.** `GET /me/notifications`.

**Mutations.** "Mark all as read" → `POST /me/notifications/read` when that route exists. Until it does, the button can stay inert. Do not clear the list locally and lose the fixture-equivalent data.

Action link: `/invite/card/{slug}?join=` for a signature ask, `/card/{slug}` for locked and payout rows.

**States.** Loading: title stays, lists wait. Empty filter: existing sentence. Error: 401 → `/signup`. Success: featured plus earlier.

**Local vs refetch.** Chips filter locally. Refetch on mount and after mark-read.

### Signup — `/signup`

Covered in [section 2](#2-auth-model). Onboarding slides do not call the API. The blurred `HomeView` behind them keeps using `emptyHomeData` so the slide does not depend on the network. Swap the name inside that backdrop only if it is free; it is out of scope to make onboarding fetch.

---

## 5. Card lifecycle and how pills map

Two axes. The list pill is `SplitCard.status`. The status screen is `LockedCardPhase`. The server card status is a third, smaller set.

### Server status (already stored)

| `cards.status` / computed | Meaning |
| --- | --- |
| `draft` | Canonical JSON stored, not every member has signed |
| `stale` | Draft whose `supersedes` is no longer the team's active card. It will never activate |
| `active` | Fully signed. `activated_at` set. This is "locked" |
| `superseded` | Replaced by a newer active card. Hide it. The UI has no superseded pill |

### Product phase (what the screens branch on)

Derive it. Do not accept it from the query string.

| Product state | List pill `SplitCard.status` | List `phase` (only to choose the detail layout) | `/card/[id]` `LockedCardPhase` | Who can open `/card/[id]` |
| --- | --- | --- | --- | --- |
| `draft` | `draft` | omit | no status screen. Row is not a link | nobody |
| `needs signature` | `draft` on the row if a card exists. The yellow strip is separate (`SignatureCard` / home attention), not a pill | omit | no | the strip links to join, not to `/card` |
| `locked` / `waiting-for-prize` | `locked` | `waiting-for-prize` | `waiting-for-prize` | yes |
| `prize-set-waiting` | `locked` | `prize-set-waiting` | `prize-set-waiting` | yes |
| `prize-in-card` | `locked` | `prize-in-card` | `prize-in-card` | yes |
| `payouts-sent` | `locked` | `prize-in-card` | `prize-in-card` | yes. This name is a query alias of prize-in-card, see below |
| `payouts-in-progress` | `paid-out` | `payouts-in-progress` | `payouts-in-progress` | yes. The Team 42 mock uses the Paid out pill while 2 of 3 are paid |
| `fully-paid-out` | `paid-out` | `payouts-in-progress` | `payouts-in-progress` with every item `paid` and `paid === total` | yes. There is no ninth layout. History uses this state |
| `cancelled` | `cancelled` | omit | no status screen | nobody. History shows it |

Derivation:

```
if team.cancelled:            list cancelled, history cancelled
else if no card or card.draft or card.stale:
                              list draft
else if payout.items all paid and items.length > 0:
                              list paid-out, detail phase payouts-in-progress
else if some payout item paid or status paying:
                              list paid-out, detail phase payouts-in-progress
else if prize_received_at:   list locked, detail phase prize-in-card
else if prize_known_at:      list locked, detail phase prize-set-waiting
else:                         list locked, detail phase waiting-for-prize
```

A missing payout route means every active card is `waiting-for-prize`. That is the correct one-shot default. Do not invent `410,000` NGN.

Header label from `headerVariantFor`:

| Phase | `headerVariant` | Header text (`headerLabel`) |
| --- | --- | --- |
| `waiting-for-prize` | `status` | Split card status |
| `prize-set-waiting` | `settings` | Split card settings |
| `prize-in-card` | `details` | Split card details |
| `payouts-in-progress` | `setting` | Split card setting |

`payoutNote(phase)`:

| Phase | Note |
| --- | --- |
| `waiting-for-prize` | No payouts yet. Your share will be sent when the prize arrives. |
| `prize-set-waiting` | The prize is set. Your share will be paid once the prize arrives. |
| `prize-in-card` and `payouts-in-progress` | Payouts for this split card has been sent |

Share chips (`shareChip`): label `Signed` until phase is `payouts-in-progress`. Then `Paid` or `Pending` from `payoutStatus`.

`prize-set-waiting` section headings say "Who's left to sign" and "Power share" even when everyone has signed. That is what `sectionCopy` does. Send members with `signed: true` and let the component pick the headings.

Home activity pill text is the label (`Draft`, `Locked`, `Paid out`), not the enum.

---

## 6. Endpoints and JSON

Existing routes are in `crates/splitcard-server/src/lib.rs` `router`. New routes are marked. All times are Unix seconds. All sats are integers. Percents are not stored.

Auth on existing writes is NIP-98 unless noted. CORS already allows any origin and the `Authorization` and `Content-Type` headers.

### Existing, call as-is

| Method | Path | Body | Response | Used by |
| --- | --- | --- | --- | --- |
| `GET` | `/health` | | `{ "status": "ok" }` | ops |
| `PUT` | `/me/profile` | `{ "name", "lightning_address" }` | `ProfileView` | signup, join, create |
| `GET` | `/profiles/{pubkey}` | | `ProfileView`. Address hidden unless viewer is that key or a teammate | member rows if you need a refresh |
| `POST` | `/teams` | `{ "slug", "name", "event_label" }` | 201 `TeamView` | create |
| `GET` | `/teams/{id}` | | `TeamView`. `join_code` and addresses only for a participant | join, create step 6 poll |
| `POST` | `/teams/{id}/join` | `{ "join_code" }` | `TeamView` | join |
| `PUT` | `/teams/{id}/submission` | `{ "submission_url" }` | `TeamView` | no screen asks for this. Skip |
| `POST` | `/teams/{id}/cards` | raw `Card` JSON | 201 or 200 `CardView` | create, once everyone has joined |
| `GET` | `/teams/{id}/card` | | `CardView` of the active card. 404 `no_active_card` when the team is still a draft | card page |
| `GET` | `/cards/{hash}` | | `CardView` | draft detail data, signatures list |
| `POST` | `/cards/{hash}/signatures` | raw Nostr event | 201 or 200 `CardView` | join Sign Card |
| `GET` | `/cards/{hash}/timestamp` | | `TimestampView` | optional line on the timeline. Not required to render the screen |
| `GET` | `/cards/{hash}/timestamp.ots` | | detached proof bytes | out of scope for the UI |
| `GET` | `/cards/{hash}/verify` and `/teams/{id}/verify` | | `VerificationReport` | out of scope for the UI |

`TeamView`:

```json
{
  "id": "42",
  "slug": "team-42",
  "name": "Team 42",
  "event_label": "Lagos Builders Hackathon",
  "join_code": "00112233445566778899aabbccddeeff",
  "submission_url": null,
  "active_card_hash": null,
  "created_at": 1737550980,
  "participants": [
    {
      "pubkey": "ab…64 hex…",
      "name": "Flora",
      "lightning_address": "flora@example.com",
      "joined_at": 1737550980
    }
  ]
}
```

`CardView.card` is a string. `JSON.parse` it. Big integers are why it is not nested JSON. Weights and sats in this product fit in JS safe integers (`MAX_WEIGHT` is `2^53-1`). Still parse the string rather than expecting an object.

`CardView` fields the adapter uses: `hash`, `team_id`, `status`, `card`, `created_by`, `created_at`, `activated_at`, `signing.signed`, `signing.missing`, `signing.state`. `supersede`, `timestamp`, `signatures`, and `card_event` can be ignored by the screens in this pass.

### Profile extensions

`bio` and `color` are collected and then dropped on the floor (`SignupView` TODO, `JoinView` TODO). One-shot options, in order:

1. Extend `PUT /me/profile` with optional `bio` (string) and `color` (one of the five `PROFILE_COLORS` or any `#RRGGBB`). Keep `deny_unknown_fields`. Return them on `ProfileView`. Screens that do not have a color yet keep using `AVATAR_PALETTE` / `PROFILE_COLORS` by index.
2. If the column is not added in this pass, the frontend stops sending `bio` and `color` and keeps color in component state. Signup still PUTs `name`. Join still PUTs `lightning_address`.

Do not block card signing on `bio`.

### New reads the screens cannot render without

`GET /me` (NIP-98). This is the list every tab is missing. There is no "teams for this pubkey" query today (`participants_pubkey` index exists and is unused by a route).

```json
{
  "pubkey": "ab…",
  "profile": {
    "pubkey": "ab…",
    "name": "Flora",
    "lightning_address": "flora@example.com",
    "updated_at": 1737550000
  },
  "teams": [
    {
      "team_id": "17",
      "slug": "kano-hack-night",
      "name": "Kano Hack Night",
      "event_label": "Kano Hack Night",
      "list_status": "draft",
      "phase": null,
      "signed_count": 1,
      "member_count": 3,
      "you_received_sats": null,
      "you_must_sign": false,
      "card_hash": "…",
      "updated_at": 1737550980
    },
    {
      "team_id": "18",
      "slug": "lagos-build",
      "name": "Lagos Build",
      "event_label": "Lagos Builders Hackathon",
      "list_status": "locked",
      "phase": "waiting-for-prize",
      "signed_count": 3,
      "member_count": 3,
      "you_received_sats": null,
      "you_must_sign": false,
      "card_hash": "…",
      "updated_at": 1755007380
    },
    {
      "team_id": "42",
      "slug": "team-42",
      "name": "Team 42",
      "event_label": "Lagos Builders Hackathon",
      "list_status": "paid-out",
      "phase": "payouts-in-progress",
      "signed_count": 3,
      "member_count": 3,
      "you_received_sats": 250000,
      "you_must_sign": false,
      "card_hash": "…",
      "updated_at": 1737551000
    }
  ],
  "needs_signature": {
    "slug": "bikini-bottom",
    "name": "Bikini Bottom",
    "inviter_name": "Goodness",
    "share_percent": 45.0,
    "teammate_count": 3,
    "invited_at": 1737543780,
    "join_code": "…"
  },
  "summary": {
    "received_sats": 0,
    "paid_team_count": 0,
    "cancelled_count": 0
  }
}
```

`needs_signature` is null when the viewer is not in any `signing.missing`. `share_percent` is a number the UI prints with `toFixed` or with a trailing `%` as the screen already does (`33%` on home, `33.3%` on join, `45%` on the signature card). Send the real ratio. Let the screen format it.

`GET /teams/by-slug/{slug}` (optional NIP-98). 404 `team_not_found` when the slug is unknown. Same `TeamView` shape. `join_code` still only for participants. This is how `/card/lagos-build` and `/invite/card/team-42` resolve, because `GET /teams/{id}` only parses integers.

`GET /invites/{slug}?join={join_code}` (optional NIP-98). Public card preview for the join wizard. Lightning addresses of other people stay hidden. The viewer's address is included when the viewer is authenticated and is that member. `join_code_ok` is true when the query matches. A wrong `join` is 403 `invalid_join_code` when the caller is trying to join, and a preview without addresses when they are only reading. Do not echo the stored join code.

`GET /teams/{id}/payout` (NIP-98, participant). 404 `payout_not_found` when the prize is unknown. That 404 maps to phase `waiting-for-prize`. Body is the payout object in [section 3](#payout). Adding this route is what turns a locked card into the later phases. Until it exists, the frontend treats 404 as waiting.

`GET /me/notifications` and `POST /me/notifications/read`. See [Notification](#notification). A derived implementation from `signing.missing`, `activated_at`, and payout `paid_at` is enough for the three sample kinds. Ids must be stable (`{card_hash}:invite`, `{card_hash}:locked`, `{card_hash}:payout:{pubkey}`) so mark-read can target them.

`GET /me/history`. Rows:

```json
{
  "cards": [
    {
      "id": "nairobi-sprint",
      "title": "Nairobi Sprint",
      "detail": "3 teammates",
      "amount_sats": 250000,
      "at": 1759190400,
      "status": "paid-out"
    },
    {
      "id": "accra-weekend",
      "title": "Accra Weekend",
      "detail": "A teammate left",
      "amount_sats": null,
      "at": 1754006400,
      "status": "cancelled"
    }
  ]
}
```

The adapter fills `amount`, `date`, and `month`. There is no cancelled writer. Return `"cards": []` for cancelled until a team flag exists. Do not map `superseded` to cancelled.

### Leave policy column

Optional `PUT /teams/{id}/policy` participant-only:

```json
{ "leave_policy": "nothing", "leave_keep_percent": 15 }
```

`leave_policy` is `nothing` or `smaller`. `leave_keep_percent` is a number in `0..100`, used only when policy is `smaller`. The locked and join copy reads it. Card JSON stays unchanged.

### Payout write side

Out of the UI's one-shot except as a reader. When a prize is recorded later, store `prize_sats` and compute items with `compute_payouts`. The browser pays each item over Lightning and the server stores `paid` only after a preimage check. That was the comment on the dropped `0004_payouts.sql` table. Do not accept an NWC URI. The locked screen does not trigger a payment.

### Display strings the adapter owns

The API should send structured fields. These sentences stay in the frontend, next to the types that already contain them:

- `1 of 3 signed`
- `Everyone has signed. Waiting for the prize`
- `250,000 sats paid to you`
- `Your share is 33.3%`
- `Paid to {address} when the prize arrives.`
- `If you leave before you submit, they keep 15% of the prize.`
- Timeline `"{name} created split card '{title}'"`
- Notification titles in the table above

`cardLink` display can be the path without scheme (`yourapp/card/team-42` is the mock). Production value: `{publicHost}/card/{slug}`. `StatusFooter` shares that URL.

---

## 7. Query params that go away

`/card/[id]` reads `searchParams.phase` (`string | string[]`, first value wins) and passes it to `getLockedCard`. Production loads phase from team + card + payout. Delete the read in `src/app/card/[id]/page.tsx`.

`resolvePayoutPhase` / `PHASE_QUERY` aliases that must stop changing the screen:

| Query | Phase it forces |
| --- | --- |
| `waiting`, `waiting-for-prize` | `waiting-for-prize` |
| `prize-set`, `prize-set-waiting`, `settings` | `prize-set-waiting` |
| `paid`, `prize-in-card`, `payouts-sent`, `prize-received` | `prize-in-card` |
| `payout`, `payouts-in-progress` | `payouts-in-progress` |

`lagos-build` already ignores the query. Other mock ids (`team-42-waiting`, `team-42-paid`, `team-42-received`, `team-42-payout`, `team-42-payouts`) exist only so a query or a direct path can open a fixture. They are not teams. After the swap, unknown slugs 404.

Invite query that should appear, not disappear: `?join={join_code}` on `/invite/card/{slug}`. That is a secret, not a phase. `parseInviteCode` must forward it. Do not put the join code in the slug segment; slugs are short public usernames and join codes are 32 hex characters.

No other search params are read. Cards, history, and notification chips are component state, not URLs.

---

## 8. Placeholder inventory

Remove these as data sources. They may remain as characters in copy or stickman files.

| Placeholder | Where |
| --- | --- |
| `Flora` / `Flora.` | Home greeting, create `creatorName` fallback, timeline "Flora created…", join inviter, `EQUAL_MEMBERS` |
| `Flare` | Payouts fixture only (`teamPayouts`), same color as Flora. Treat as a typo of Flora when reading mocks. Do not create a user named Flare |
| `Goodness` | Members, attention body, signature card, notification title, `KNOWN_NAMES` |
| `Spongebob Squarepants` / short `Spongebob` | Members, `spongebobeddie@pineapple.com` |
| `Favour`, `Solomonfaith` | `teamPrizeInCard` members only |
| `greedy@lion.com` | `SHARE_DESTINATION`, `DEMO_SHARE_DESTINATION`. Fallback when the join destination field is blank (`StepSign.tsx`) |
| `goodness@friedrice.com` | `DEMO_INVITE.email`, `KNOWN_NAMES` |
| `spongebobeddie@pineapple.com` | `KNOWN_NAMES` |
| `410000` / `410,000` | `PRIZE_AMOUNT`, prize labels, `SummaryCard` total (there labeled sats, on the card labeled NGN) |
| `250000` / `250,000` | Team 42 list amount (sats), Nairobi history and notification |
| `40000` / `40,000` | Abuja history |
| `33.3` and `33%` and `45%` and `30/40/30` | Share fixtures. `45%` is Bikini Bottom only. Home Orbit Pay says `33%` |
| `15` | Default `leaveKeepPercent` |
| `Bikini Bottom` | Signature card and notification `bikini-invite`. Not in `cardsSample` |
| `Orbit Pay` | Home attention title. Not in `cardsSample` |
| `Lagos Build` / `lagos-build` | List, home activity, locked fixture, notification `lagos-locked` |
| `Team 42` / `team-42` | List, home, locked fixtures, `DEMO_INVITE.code`, invite slug example |
| `Kano Hack Night` / `kano-hack-night` | Draft list row only |
| `Lagos Builders Hackathon` | `DEMO_INVITE.hackathon` |
| `Nairobi Sprint`, `Abuja Hack`, `Accra Weekend` | History and the Nairobi payout notification |
| `yourapp/card/team-42`, `yourapp/card/lagos-build` | `cardLink` display |
| Lock stamps `22 JAN, 2025` `14:03 GMT +1` and `12 AUG, 2026` `14:03 GMT +1` and created date `5 AUG, 2026` | Timeline and prize card |
| Colors `#43c7ff`, `#f78125`, `#8a38f5`, `#34b6ee`, `#ffffff`, `#c8c8c8`, `#7A5CFF`, `#F59A3A`, `#3EE06A`, `#E23B3B`, `#FF4DA6` | Member swatches and profile picker |
| `safehand.signup` | sessionStorage wizard key |
| QR value | Whatever URL step 6 last built, including a slug of `team` when the team name is empty (`inviteSlug` fallback). Step 6 is only reachable after a non-empty name, so the fallback is defensive |

OTP: none. No leftover code entry, timer, or verify route.

Demo emails are the two `KNOWN_NAMES` keys plus `DEMO_INVITE.email` (the Goodness address again) plus `greedy@lion.com`, which is a Lightning-address-shaped string sitting in an email input.

`formatSats` in `src/components/cards/locked/types.ts` formats with three fraction digits (`en-US`). Nothing in the views calls it. Payout amounts on screen are either integer labels (`410,000`) or the list sentence. Format sats with `toLocaleString("en-US")` and no fraction digits.

---

## 9. Frontend files to change

One adapter module is enough, for example `src/lib/api.ts`, called from the pages and views below. Do not redesign components. Replace the imported fixture with the fetched UI type.

| Endpoint | Stop reading | Start feeding | Types |
| --- | --- | --- | --- |
| `PUT /me/profile`, local key | `src/components/signup/SignupView.tsx` TODOs, `SignupWelcome.tsx`, `SignupProfile.tsx` | same files, on the existing buttons | `SignupState` |
| `GET /me` | `activityHomeData` in `src/app/page.tsx` | `HomeView` `data` | `HomeData`, `HomeAttention`, `HomeActivityItem` |
| `GET /me` | `cardsSample` in `src/components/cards/CardsView.tsx` | `cardsForFilter(filter, cards)` | `SplitCard` |
| `GET /me` `summary` | hardcoded numbers in `src/components/cards/SummaryCard.tsx` | props | new props, same markup |
| `GET /me` `needs_signature` | hardcoded copy in `src/components/cards/SignatureCard.tsx` | props, `Sign now` links to the invite | |
| `GET /me` `needs_signature` | `AttentionCard` button has no href | `href` on the button or a `Link` | `HomeAttention` |
| `GET /teams/by-slug/{slug}` + card + payout | `getLockedCard` in `src/app/card/[id]/page.tsx` | `LockedCardStatusView card` | `LockedSplitCardDetail` |
| phase query | `searchParams` on that page, `resolvePayoutPhase`, `lockedCards`, `generateStaticParams` | delete the query. Fetch by slug. Drop static params or they 404 real slugs | |
| Share URL | `StatusFooter` in `LockedCardStatusView.tsx` | `card.cardLink` from the payload | already a prop |
| `POST /teams`, `POST /teams/{id}/cards`, `POST /cards/{hash}/signatures` | local-only `goNext` in `src/components/create/CreateView.tsx` | call on Save Draft and Send Invites | `WizardState` → `Card` |
| Invite URL | `StepInvite.tsx`, `invitePath` | append `?join=` | |
| `GET /invites/{slug}` | `DEMO_INVITE` in `JoinView.tsx`, `StepInviteCard.tsx`, `StepSign.tsx`, `StepSigned.tsx` | props | `JoinMember`, invite fields |
| `PUT /me/profile` + signature POST | `JoinView` `goNext` TODO | step 2 and step 3 | `JoinState` |
| `parseInviteCode` fallback | `src/components/join/types.ts`, `ScanView.tsx` | return empty and do not navigate | |
| `GET /me/history` | `historySample` in `HistoryView.tsx` | `historyForFilter` | `HistoryCard` |
| `GET /me/notifications` | `notificationsSample` in `NotificationsView.tsx` | `notificationsForFilter` | `AppNotification` |
| Featured action | `FeaturedNotification` `href="/cards"` | invite or card href from the item | add optional `href` on `AppNotification` |

Files that stay fixtures on purpose:

- `src/components/signup/SignupOnboarding.tsx` may keep importing `emptyHomeData` for the blurred backdrop.
- `src/app/demo/home-activity/page.tsx` may keep `activityHomeData`. It is a demo route, not a product route.
- `src/components/home/types.ts` keeps `emptyHomeData` as the empty shape the home page fills with a real name.

`src/components/cards/types.ts` `cardsForFilter`, `src/components/history/types.ts` `historyForFilter` and `groupHistoryByMonth`, and `src/components/notifications/types.ts` `notificationsForFilter` stay. They are the local filters.

---

## 10. Integration order

Do these in order. Each step is visible in the existing UI. Later steps may 404 into the empty states already on the screens.

1. **Auth.** Local key, NIP-98 helper, `PUT /me/profile` from signup profile submit and from join step 2. Sign-in restores the key. `auth_missing` on any page opens `/signup`. Confirm `GET /profiles/{pubkey}` shows the name you saved.
2. **Cards list and home.** Add `GET /me`. Point `/` and `/cards` at it. Empty account uses the `emptyHomeData` layout with the real name. Summary and signature strip read `summary` and `needs_signature`. Filters stay local. Draft rows stay non-links.
3. **Detail.** Add `GET /teams/by-slug/{slug}`. `/card/[id]` loads the active card and maps `waiting-for-prize` when payout 404s. Remove `?phase=` and `generateStaticParams`. Share uses the real URL. `lagos-build` and `team-42` 404 unless those teams exist.
4. **Create.** `POST /teams` on Save Draft and on Send Invites. Step 6 shows `/invite/card/{slug}?join={join_code}` and the client QR. Submit and sign the card only after participants exist. The new team shows on `/cards` as a draft.
5. **Join.** Invite page reads the slug and `join`. Replace `DEMO_INVITE`. `POST /teams/{id}/join`, then `POST /cards/{hash}/signatures`. Already-signed viewers land on step 4. Empty scan paste does not open `team-42`.
6. **Payouts.** Add `GET /teams/{id}/payout`. When it returns items, the same `/card/[id]` view switches phase using the table in section 5. List pill becomes `paid-out` with `you_received_sats`. No payout route yet: leave every locked card on `waiting-for-prize`.
7. **Notifications and history.** `GET /me/notifications` and `GET /me/history`. Wire the featured action href. Mark-all-read can follow. History stays empty until a payout is fully paid or a team is cancelled. That matches `No cards in this filter.`

Verify by walking the phone shell: signup → empty home → create team → cards list shows the draft → second browser joins the invite URL → both sign → card opens on waiting-for-prize → history and notifications show the lock event and, once a payout exists, the paid row.

---

## 11. Out of scope

Still client-only. Do not block the API on these.

- Onboarding copy and slide order (`ONBOARDING_SLIDES` in `src/components/signup/types.ts`).
- Stickman SVGs under `public/dark-stickman/` and `public/white-stickman/`, and every `<Image>` that places them.
- Filter chip markup and selected styles on cards, history, and notifications. Only the arrays they filter change.
- `PhoneShell` width, bottom nav icons, hamburger button (no menu exists).
- Bell dot painting. It is always on.
- QR drawing (`src/components/create/qr.tsx`) and the scan camera icon. Paste remains the way in.
- The three-slot share bar (`WhoGetsWhat`). Pass all members through. A two-person team simply hides that bar.
- `CardList` empty sentence `"No cancelled cards."` for every empty filter.
- Header typos the components already encode (`Split card setting`, `Payouts for this split card has been sent`).
- Superseding a locked card, peer review (`rules.peer_review`), submission URL, verify report, and the `.ots` download. No screen calls them.
- Paying invoices from the locked screen. NWC URIs stay off the server.
- Reloading `/create` with a saved draft.
- Rewriting `/demo/home-activity`.
- NGN amounts. The server and `compute_payouts` are sats. The `NGN` unit on `teamPrizeSet` and `teamPrizeInCard` is fixture text. Responses use `sats`. The `currency` field on `SplitCard` may stay `"sats"`.
- Making `formatSats` (three decimal places) the display format.

---

## 12. TODOs already in the code

These are the comments to close while wiring. Quoted from the files.

`src/app/card/[id]/page.tsx`

- Load the split card, prize detection, and payout execution from the cards API. Replace `getLockedCard(id, phase)`.

`src/components/cards/locked/LockedCardStatusView.tsx`

- Share the canonical card URL from the API, not the mock slug.

`src/components/cards/locked/types.ts`

- Replace `payoutSats` with the payout API amount for this member.
- Prize total comes from prize detection, not the mock (`amount`).
- Payout destination is the one saved with the signature, not `greedy@lion.com`.
- Payout execution: paid vs pending must come from the payout API (`teamPayouts`).
- Prize detection moves a locked card from `waiting-for-prize` to `prize-set-waiting` when the prize lands.
- Payout execution fills paid/pending and the in-card total.
- History and notifications read these payout events from the API instead of `history/types` and `notifications/types`.

`src/components/cards/types.ts`

- `SplitCard.amount` is the payout amount from the payouts API.

`src/components/history/types.ts`

- History should list payout events from the payouts API, not `historySample`.

`src/components/notifications/types.ts`

- Notifications should include prize-detected and payout events from the API.

`src/components/join/JoinView.tsx`

- Persist profile colour, payout destination, and signature via the split-card API.

`src/components/join/StepSign.tsx`

- Show the payout destination saved with the signature, not `DEMO_SHARE_DESTINATION`.

`src/components/signup/SignupView.tsx`

- POST display name and bio to the registration API.
- Create a local login / backup key before collecting the profile.

`src/components/signup/SignupWelcome.tsx`

- Wire Nostr signup (NIP-07 / nsec) to the auth backend.
- Wire Bitcoin wallet signup (lnurl / nwc / on-chain) to the auth backend. The server rejects NWC connection strings in bodies. Wallet signup means a local key, not a posted secret.
- Wire returning-user sign-in (session, Nostr, or wallet).

`src/components/signup/SignupProfile.tsx`

- Same returning-user sign-in TODO on the profile sign-in stub.

No OTP TODO exists.
