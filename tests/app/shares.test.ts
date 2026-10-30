import assert from "node:assert/strict";
import { test } from "node:test";
import {
  equalTenths,
  formatBasisPoints,
  percentToTenths,
  tenthsToPercent,
  weightsFor,
} from "@/lib/shares";
import wasm from "./wasm";

test("typed percents become whole tenths, with no floats", () => {
  assert.equal(percentToTenths("33.3"), 333);
  assert.equal(percentToTenths(" 100 "), 1000);
  assert.equal(percentToTenths("0.1"), 1);
  for (const bad of ["", "33.33", "1e2", "-5", "100.1", "abc", "33,3", ".5"]) {
    assert.equal(percentToTenths(bad), null, bad);
  }
  assert.equal(tenthsToPercent(334), "33.4");
  assert.equal(tenthsToPercent(1000), "100.0");
});

test("equal splits weigh 1 each; custom splits weigh tenths totalling 100.0%", () => {
  assert.deepEqual(weightsFor("equal", ["", "", ""]), { weights: [1, 1, 1] });
  assert.deepEqual(weightsFor("custom", ["33.4", "33.3", "33.3"]), { weights: [334, 333, 333] });
  assert.deepEqual(weightsFor("custom", ["50", "49.9"]), { problem: "wrong_total" });
  assert.deepEqual(weightsFor("custom", ["100", "0"]), { problem: "zero_share" });
  assert.deepEqual(weightsFor("custom", ["50", "fifty"]), { problem: "not_a_percent" });
  assert.deepEqual(equalTenths(3), [334, 333, 333]);
  assert.equal(equalTenths(7).reduce((sum, value) => sum + value, 0), 1000);
});

test("shown percentages come from splitcard-core's basis points", () => {
  const pubkeys = [
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5",
    "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
  ];
  const card = wasm.new_card({
    card_id: "t",
    event: "e",
    team: "t",
    members: pubkeys.map((pubkey) => ({ pubkey, weight: 1 })),
  });
  const basisPoints = wasm.display_basis_points(card);
  assert.deepEqual(basisPoints.map(formatBasisPoints), ["33.3%", "33.3%", "33.3%"]);
  assert.equal(formatBasisPoints(3334), "33.3%");
  assert.equal(formatBasisPoints(3335), "33.4%");
});
