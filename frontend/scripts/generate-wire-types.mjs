import { compile } from "json-schema-to-typescript";
import { readFile, writeFile, readdir } from "node:fs/promises";
const directory = new URL("../src/generated/", import.meta.url);
const schemaDirectory = new URL("schema/", directory);
for (const name of (await readdir(schemaDirectory)).filter((name) => name.endsWith(".json")).sort()) {
  const schema = JSON.parse(await readFile(new URL(name, schemaDirectory), "utf8"));
  const output = await compile(schema, name.slice(0, -5), {
    bannerComment: "/* Generated from Rust public DTOs. Run npm run contracts:generate. */",
    additionalProperties: false,
    style: { semi: true, doubleQuote: true },
  });
  await writeFile(new URL(name.replace(/\.json$/, ".ts"), directory), output);
}
