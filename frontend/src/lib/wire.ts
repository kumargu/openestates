import Ajv, { type ValidateFunction } from "ajv";
import journeySchema from "../generated/schema/SearchJourneyEnvelope.json" with { type: "json" };
import proofSchema from "../generated/schema/SearchProofResolution.json" with { type: "json" };
import detailSchema from "../generated/schema/PropertyDetail.json" with { type: "json" };
import summarySchema from "../generated/schema/PropertySummaries.json" with { type: "json" };
import contextSchema from "../generated/schema/PropertyContext.json" with { type: "json" };

import proofFailureSchema from "../generated/schema/SearchProofFailure.json" with { type: "json" };

import catalogSchema from "../generated/schema/PropertyCatalog.json" with { type: "json" };
import evidenceSchema from "../generated/schema/PropertyEvidence.json" with { type: "json" };

const ajv = new Ajv({ strict: false, validateFormats: false });
const validators: Record<string, ValidateFunction> = {
  catalog: ajv.compile(catalogSchema), evidence: ajv.compile(evidenceSchema),
  journey: ajv.compile(journeySchema), proof: ajv.compile(proofSchema), proofFailure: ajv.compile(proofFailureSchema),
  detail: ajv.compile(detailSchema), summaries: ajv.compile(summarySchema), context: ajv.compile(contextSchema),
};

/** Decode the real wire response before it enters presentation state or a cache. */
export function validateWire<T>(kind: keyof typeof validators, payload: unknown): T {
  const validate = validators[kind];
  if (!validate(payload)) throw new Error(`Invalid ${kind} response: ${ajv.errorsText(validate.errors)}`);
  return payload as T;
}
