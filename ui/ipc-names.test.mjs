import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const names = JSON.parse(readFileSync(new URL("./src/__fixtures__/ipc-names.json", import.meta.url), "utf8"));
const catalog = readFileSync(new URL("./src/catalog.ts", import.meta.url), "utf8");
const api = readFileSync(new URL("./src/api.ts", import.meta.url), "utf8");

function stringUnion(source, typeName) {
  const match = source.match(new RegExp(`export type ${typeName} =([\\s\\S]*?);`));
  assert.ok(match, typeName);
  const values = [...match[1].matchAll(/"([a-z0-9_]+)"/g)].map((item) => item[1]).sort();
  assert.ok(values.length > 0, typeName);
  return values;
}

assert.deepEqual(names.tool_id, stringUnion(catalog, "ToolId"));
assert.deepEqual(names.capability, stringUnion(catalog, "CapabilityId"));
assert.deepEqual(names.tool_presence, stringUnion(catalog, "Presence"));
assert.deepEqual(names.grant_status, stringUnion(api, "GrantStatus"));
assert.deepEqual(names.summary_mode, stringUnion(api, "SummaryMode"));
assert.deepEqual(names.appearance, stringUnion(api, "Appearance"));
assert.deepEqual(names.source_status, stringUnion(api, "SourceStatus"));
