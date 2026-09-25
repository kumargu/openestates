import Ajv from "ajv";
import catalogSchema from "./generated/schema/InventoryCatalog.json";
import detailSchema from "./generated/schema/InventoryDetail.json";
import type { InventoryCatalog } from "./generated/InventoryCatalog.ts";
import type { InventoryDetail } from "./generated/InventoryDetail.ts";

const ajv = new Ajv({ strict: false });
const catalogValidator = ajv.compile<InventoryCatalog>(catalogSchema);
const detailValidator = ajv.compile<InventoryDetail>(detailSchema);

async function read(path: string, signal: AbortSignal): Promise<unknown> {
  const response = await fetch(path, { signal: AbortSignal.any([signal, AbortSignal.timeout(8000)]) });
  if (response.status === 409) throw new Error("These observations changed. Return to results to reload them.");
  if (!response.ok) throw new Error("Asking prices couldn’t be loaded.");
  return response.json();
}

export async function getCatalog(signal: AbortSignal): Promise<InventoryCatalog> {
  const data = await read("/api/inventory/homes", signal);
  if (!catalogValidator(data) || data.contract_version !== 1 || !data.fixture) throw new Error("The preview data couldn’t be read.");
  return data;
}

export async function getDetail(id: string, snapshot: string, signal: AbortSignal): Promise<InventoryDetail> {
  const data = await read(`/api/inventory/homes/${encodeURIComponent(id)}?snapshot=${encodeURIComponent(snapshot)}`, signal);
  if (!detailValidator(data) || data.contract_version !== 1 || !data.fixture || data.snapshot_id !== snapshot || data.summary.home.id !== id) throw new Error("The observations don’t match this home.");
  return data;
}
