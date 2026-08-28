import test from "node:test"; import assert from "node:assert/strict";
import { fakeEvents, runFakeHost } from "../public/app.mjs";
test("renders the Host API v1 fake session", () => assert.equal(runFakeHost(), "hello from Garive"));
test("rejects EOF without terminal", () => assert.throws(() => runFakeHost(fakeEvents.slice(0, -1)), /terminal/));
