import { useEffect, useState, useCallback } from "react";
import { Helmet } from "react-helmet-async";
import { Link } from "react-router-dom";
import type { RegistrationDraft, Step1Payload, Step2Payload, Step3Payload, Step4Payload, Step5Payload, Step6Payload, Step7Payload, PublishResult } from "../lib/types.ts";
import { createRegistration, getRegistration, updateRegistrationStep, publishRegistration } from "../lib/api.ts";

const TOTAL_STEPS = 7;
const DRAFT_ID_KEY = "oe_registration_draft_id";

const STEP_LABELS = [
  "Basic Info",
  "Property Prompt",
  "Property Details",
  "Pricing",
  "Documents",
  "Photos",
  "Society Info",
];

function ProgressBar({ currentStep }: { currentStep: number }) {
  return (
    <div style={{ marginBottom: "2rem" }}>
      <div style={{ display: "flex", gap: "0.35rem", marginBottom: "0.5rem" }}>
        {STEP_LABELS.map((label, i) => {
          const stepNum = i + 1;
          const isCompleted = stepNum <= currentStep;
          const isCurrent = stepNum === currentStep + 1;
          return (
            <div key={label} style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  height: "4px",
                  borderRadius: "2px",
                  backgroundColor: isCompleted
                    ? "#c96b4f"
                    : isCurrent
                    ? "rgba(201,107,79,0.35)"
                    : "rgba(0,0,0,0.08)",
                  transition: "background-color 0.3s ease",
                }}
              />
              <p
                style={{
                  margin: "0.35rem 0 0",
                  fontSize: "0.7rem",
                  color: isCompleted || isCurrent ? "#1a1a1a" : "#bbb",
                  fontWeight: isCurrent ? 600 : 400,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {label}
              </p>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function Step1Form({
  draft,
  onSave,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step1Payload) => void;
  saving: boolean;
}) {
  const [name, setName] = useState(draft.name ?? "");
  const [email, setEmail] = useState(draft.email ?? "");
  const [phone, setPhone] = useState(draft.phone ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const validate = useCallback((): boolean => {
    const errs: Record<string, string> = {};
    const trimmedName = name.trim();
    if (!trimmedName) {
      errs.name = "Name is required";
    } else if (trimmedName.length > 100) {
      errs.name = "Name must be 100 characters or fewer";
    }
    const trimmedEmail = email.trim();
    if (trimmedEmail && (!trimmedEmail.includes("@") || !trimmedEmail.includes("."))) {
      errs.email = "Enter a valid email address";
    }
    const digits = phone.replace(/[^0-9]/g, "");
    if (phone.trim() && (digits.length < 10 || digits.length > 15)) {
      errs.phone = "Phone must be 10-15 digits";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }, [name, email, phone]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const payload: Step1Payload = { name: name.trim() };
    if (email.trim()) payload.email = email.trim();
    if (phone.trim()) payload.phone = phone.trim();
    onSave(payload);
  };

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Basic Information
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Tell us a bit about yourself. Only your name is required.
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={{ display: "block", fontSize: "0.85rem", fontWeight: 600, marginBottom: "0.35rem", color: "#333" }}>
          Name <span style={{ color: "#c96b4f" }}>*</span>
        </label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Your full name"
          maxLength={100}
          style={{
            width: "100%",
            padding: "0.7rem 0.85rem",
            border: errors.name ? "1px solid #ef4444" : "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            outline: "none",
            transition: "border-color 0.2s",
            boxSizing: "border-box",
          }}
          onFocus={(e) => { if (!errors.name) e.currentTarget.style.borderColor = "#c96b4f"; }}
          onBlur={(e) => { if (!errors.name) e.currentTarget.style.borderColor = "rgba(0,0,0,0.12)"; }}
        />
        {errors.name && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.name}</p>
        )}
      </div>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={{ display: "block", fontSize: "0.85rem", fontWeight: 600, marginBottom: "0.35rem", color: "#333" }}>
          Email <span style={{ color: "#999", fontWeight: 400, fontSize: "0.8rem" }}>(optional)</span>
        </label>
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@example.com"
          style={{
            width: "100%",
            padding: "0.7rem 0.85rem",
            border: errors.email ? "1px solid #ef4444" : "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            outline: "none",
            transition: "border-color 0.2s",
            boxSizing: "border-box",
          }}
          onFocus={(e) => { if (!errors.email) e.currentTarget.style.borderColor = "#c96b4f"; }}
          onBlur={(e) => { if (!errors.email) e.currentTarget.style.borderColor = "rgba(0,0,0,0.12)"; }}
        />
        {errors.email && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.email}</p>
        )}
      </div>

      <div style={{ marginBottom: "1.75rem" }}>
        <label style={{ display: "block", fontSize: "0.85rem", fontWeight: 600, marginBottom: "0.35rem", color: "#333" }}>
          Phone <span style={{ color: "#999", fontWeight: 400, fontSize: "0.8rem" }}>(optional)</span>
        </label>
        <input
          type="tel"
          value={phone}
          onChange={(e) => setPhone(e.target.value)}
          placeholder="+91 98765 43210"
          style={{
            width: "100%",
            padding: "0.7rem 0.85rem",
            border: errors.phone ? "1px solid #ef4444" : "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            outline: "none",
            transition: "border-color 0.2s",
            boxSizing: "border-box",
          }}
          onFocus={(e) => { if (!errors.phone) e.currentTarget.style.borderColor = "#c96b4f"; }}
          onBlur={(e) => { if (!errors.phone) e.currentTarget.style.borderColor = "rgba(0,0,0,0.12)"; }}
        />
        {errors.phone && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.phone}</p>
        )}
      </div>

      <button
        type="submit"
        disabled={saving}
        style={{
          padding: "0.7rem 1.5rem",
          backgroundColor: saving ? "#ccc" : "#c96b4f",
          color: "#fff",
          border: "none",
          borderRadius: "8px",
          fontSize: "0.95rem",
          fontWeight: 600,
          cursor: saving ? "not-allowed" : "pointer",
          transition: "background-color 0.2s",
        }}
      >
        {saving ? "Saving..." : "Save & Continue"}
      </button>
    </form>
  );
}

function Step2Form({
  draft,
  onSave,
  onBack,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step2Payload) => void;
  onBack: () => void;
  saving: boolean;
}) {
  const [prompt, setPrompt] = useState(draft.property_prompt ?? "");
  const [error, setError] = useState("");

  const validate = useCallback((): boolean => {
    const trimmed = prompt.trim();
    if (!trimmed) {
      setError("Property description is required");
      return false;
    }
    if (trimmed.length > 500) {
      setError("Must be 500 characters or fewer");
      return false;
    }
    setError("");
    return true;
  }, [prompt]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    onSave({ property_prompt: prompt.trim() });
  };

  const charCount = prompt.length;
  const isOverLimit = charCount > 500;

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Property Description
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Describe your property in your own words. What makes it special? What should buyers know?
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={{ display: "block", fontSize: "0.85rem", fontWeight: 600, marginBottom: "0.35rem", color: "#333" }}>
          Your property prompt <span style={{ color: "#c96b4f" }}>*</span>
        </label>
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          placeholder="e.g. Spacious 3BHK in Prestige Lakeside Habitat with a lake-facing balcony. Recently renovated kitchen. Walking distance to Wipro campus..."
          rows={6}
          maxLength={550}
          style={{
            width: "100%",
            padding: "0.7rem 0.85rem",
            border: error ? "1px solid #ef4444" : "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            outline: "none",
            resize: "vertical",
            fontFamily: "inherit",
            lineHeight: 1.5,
            transition: "border-color 0.2s",
            boxSizing: "border-box",
          }}
          onFocus={(e) => { if (!error) e.currentTarget.style.borderColor = "#c96b4f"; }}
          onBlur={(e) => { if (!error) e.currentTarget.style.borderColor = "rgba(0,0,0,0.12)"; }}
        />
        <div style={{ display: "flex", justifyContent: "space-between", marginTop: "0.3rem" }}>
          {error ? (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: 0 }}>{error}</p>
          ) : (
            <span />
          )}
          <span
            style={{
              fontSize: "0.78rem",
              color: isOverLimit ? "#ef4444" : charCount > 450 ? "#f59e0b" : "#aaa",
              fontWeight: isOverLimit ? 600 : 400,
            }}
          >
            {charCount}/500
          </span>
        </div>
      </div>

      <div style={{ display: "flex", gap: "0.75rem" }}>
        <button
          type="button"
          onClick={onBack}
          style={{
            padding: "0.7rem 1.5rem",
            backgroundColor: "transparent",
            color: "#666",
            border: "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Back
        </button>
        <button
          type="submit"
          disabled={saving}
          style={{
            padding: "0.7rem 1.5rem",
            backgroundColor: saving ? "#ccc" : "#c96b4f",
            color: "#fff",
            border: "none",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 600,
            cursor: saving ? "not-allowed" : "pointer",
            transition: "background-color 0.2s",
          }}
        >
          {saving ? "Saving..." : "Save & Continue"}
        </button>
      </div>
    </form>
  );
}

const PROPERTY_TYPES = [
  { value: "apartment", label: "Apartment" },
  { value: "villa", label: "Villa" },
  { value: "plot", label: "Plot" },
  { value: "independent_house", label: "Independent House" },
];

const FACING_OPTIONS = [
  { value: "", label: "Select facing" },
  { value: "north", label: "North" },
  { value: "south", label: "South" },
  { value: "east", label: "East" },
  { value: "west", label: "West" },
  { value: "north_east", label: "North East" },
  { value: "north_west", label: "North West" },
  { value: "south_east", label: "South East" },
  { value: "south_west", label: "South West" },
];

const FURNISHING_OPTIONS = [
  { value: "", label: "Select furnishing" },
  { value: "furnished", label: "Furnished" },
  { value: "semi_furnished", label: "Semi Furnished" },
  { value: "unfurnished", label: "Unfurnished" },
];

const inputStyle = (hasError: boolean) => ({
  width: "100%",
  padding: "0.7rem 0.85rem",
  border: hasError ? "1px solid #ef4444" : "1px solid rgba(0,0,0,0.12)",
  borderRadius: "8px",
  fontSize: "0.95rem",
  outline: "none",
  transition: "border-color 0.2s",
  boxSizing: "border-box" as const,
});

const labelStyle = { display: "block" as const, fontSize: "0.85rem", fontWeight: 600, marginBottom: "0.35rem", color: "#333" };

function Step3Form({
  draft,
  onSave,
  onBack,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step3Payload) => void;
  onBack: () => void;
  saving: boolean;
}) {
  const existing = (draft.property_details ?? {}) as Partial<Step3Payload>;
  const [propertyType, setPropertyType] = useState(existing.property_type ?? "");
  const [bhk, setBhk] = useState(existing.bhk?.toString() ?? "");
  const [carpetArea, setCarpetArea] = useState(existing.carpet_area_sqft?.toString() ?? "");
  const [floor, setFloor] = useState(existing.floor?.toString() ?? "");
  const [totalFloors, setTotalFloors] = useState(existing.total_floors?.toString() ?? "");
  const [facing, setFacing] = useState(existing.facing ?? "");
  const [furnishing, setFurnishing] = useState(existing.furnishing ?? "");
  const [ageYears, setAgeYears] = useState(existing.age_years?.toString() ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const needsBhk = propertyType === "apartment" || propertyType === "villa";

  const validate = useCallback((): boolean => {
    const errs: Record<string, string> = {};
    if (!propertyType) errs.property_type = "Property type is required";
    if (needsBhk) {
      const b = parseInt(bhk);
      if (!bhk || isNaN(b)) errs.bhk = "BHK is required for this property type";
      else if (b < 1 || b > 6) errs.bhk = "BHK must be 1-6";
    }
    if (carpetArea) {
      const a = parseInt(carpetArea);
      if (isNaN(a) || a < 100 || a > 50000) errs.carpet_area = "Area must be 100-50,000 sq ft";
    }
    if (floor) {
      const f = parseInt(floor);
      if (isNaN(f) || f < 0 || f > 99) errs.floor = "Floor must be 0-99";
    }
    if (totalFloors) {
      const t = parseInt(totalFloors);
      if (isNaN(t) || t < 1 || t > 99) errs.total_floors = "Total floors must be 1-99";
    }
    if (ageYears) {
      const a = parseInt(ageYears);
      if (isNaN(a) || a < 0 || a > 99) errs.age_years = "Age must be 0-99 years";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }, [propertyType, bhk, carpetArea, floor, totalFloors, ageYears, needsBhk]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const payload: Step3Payload = { property_type: propertyType };
    if (needsBhk && bhk) payload.bhk = parseInt(bhk);
    if (carpetArea) payload.carpet_area_sqft = parseInt(carpetArea);
    if (floor) payload.floor = parseInt(floor);
    if (totalFloors) payload.total_floors = parseInt(totalFloors);
    if (facing) payload.facing = facing;
    if (furnishing) payload.furnishing = furnishing;
    if (ageYears) payload.age_years = parseInt(ageYears);
    onSave(payload);
  };

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Property Details
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Help buyers find your property with accurate details.
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>
          Property Type <span style={{ color: "#c96b4f" }}>*</span>
        </label>
        <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
          {PROPERTY_TYPES.map((pt) => (
            <button
              key={pt.value}
              type="button"
              onClick={() => setPropertyType(pt.value)}
              style={{
                padding: "0.55rem 1rem",
                border: propertyType === pt.value ? "2px solid #c96b4f" : "1px solid rgba(0,0,0,0.12)",
                borderRadius: "8px",
                backgroundColor: propertyType === pt.value ? "rgba(201,107,79,0.06)" : "#fff",
                color: propertyType === pt.value ? "#c96b4f" : "#555",
                fontWeight: propertyType === pt.value ? 600 : 400,
                fontSize: "0.9rem",
                cursor: "pointer",
                transition: "all 0.2s",
              }}
            >
              {pt.label}
            </button>
          ))}
        </div>
        {errors.property_type && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.property_type}</p>
        )}
      </div>

      {needsBhk && (
        <div style={{ marginBottom: "1.25rem" }}>
          <label style={labelStyle}>
            BHK <span style={{ color: "#c96b4f" }}>*</span>
          </label>
          <div style={{ display: "flex", gap: "0.5rem" }}>
            {[1, 2, 3, 4, 5, 6].map((n) => (
              <button
                key={n}
                type="button"
                onClick={() => setBhk(n.toString())}
                style={{
                  width: "44px",
                  height: "44px",
                  border: bhk === n.toString() ? "2px solid #c96b4f" : "1px solid rgba(0,0,0,0.12)",
                  borderRadius: "8px",
                  backgroundColor: bhk === n.toString() ? "rgba(201,107,79,0.06)" : "#fff",
                  color: bhk === n.toString() ? "#c96b4f" : "#555",
                  fontWeight: bhk === n.toString() ? 600 : 400,
                  fontSize: "0.95rem",
                  cursor: "pointer",
                  transition: "all 0.2s",
                }}
              >
                {n}
              </button>
            ))}
          </div>
          {errors.bhk && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.bhk}</p>
          )}
        </div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1rem", marginBottom: "1.25rem" }}>
        <div>
          <label style={labelStyle}>Carpet Area (sq ft)</label>
          <input
            type="number"
            value={carpetArea}
            onChange={(e) => setCarpetArea(e.target.value)}
            placeholder="e.g. 1200"
            style={inputStyle(!!errors.carpet_area)}
          />
          {errors.carpet_area && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.carpet_area}</p>
          )}
        </div>
        <div>
          <label style={labelStyle}>Age (years)</label>
          <input
            type="number"
            value={ageYears}
            onChange={(e) => setAgeYears(e.target.value)}
            placeholder="e.g. 5"
            style={inputStyle(!!errors.age_years)}
          />
          {errors.age_years && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.age_years}</p>
          )}
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1rem", marginBottom: "1.25rem" }}>
        <div>
          <label style={labelStyle}>Floor</label>
          <input
            type="number"
            value={floor}
            onChange={(e) => setFloor(e.target.value)}
            placeholder="e.g. 7"
            style={inputStyle(!!errors.floor)}
          />
          {errors.floor && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.floor}</p>
          )}
        </div>
        <div>
          <label style={labelStyle}>Total Floors</label>
          <input
            type="number"
            value={totalFloors}
            onChange={(e) => setTotalFloors(e.target.value)}
            placeholder="e.g. 20"
            style={inputStyle(!!errors.total_floors)}
          />
          {errors.total_floors && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.total_floors}</p>
          )}
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1rem", marginBottom: "1.75rem" }}>
        <div>
          <label style={labelStyle}>Facing</label>
          <select
            value={facing}
            onChange={(e) => setFacing(e.target.value)}
            style={{ ...inputStyle(false), appearance: "auto" as const }}
          >
            {FACING_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
        </div>
        <div>
          <label style={labelStyle}>Furnishing</label>
          <select
            value={furnishing}
            onChange={(e) => setFurnishing(e.target.value)}
            style={{ ...inputStyle(false), appearance: "auto" as const }}
          >
            {FURNISHING_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
        </div>
      </div>

      <div style={{ display: "flex", gap: "0.75rem" }}>
        <button
          type="button"
          onClick={onBack}
          style={{
            padding: "0.7rem 1.5rem",
            backgroundColor: "transparent",
            color: "#666",
            border: "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Back
        </button>
        <button
          type="submit"
          disabled={saving}
          style={{
            padding: "0.7rem 1.5rem",
            backgroundColor: saving ? "#ccc" : "#c96b4f",
            color: "#fff",
            border: "none",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 600,
            cursor: saving ? "not-allowed" : "pointer",
            transition: "background-color 0.2s",
          }}
        >
          {saving ? "Saving..." : "Save & Continue"}
        </button>
      </div>
    </form>
  );
}

const POSSESSION_OPTIONS = [
  { value: "", label: "Select status" },
  { value: "ready", label: "Ready to Move" },
  { value: "under_construction", label: "Under Construction" },
  { value: "resale", label: "Resale" },
];

function formatPrice(value: string): string {
  const num = value.replace(/[^0-9]/g, "");
  if (!num) return "";
  return parseInt(num).toLocaleString("en-IN");
}

function parsePriceInput(formatted: string): string {
  return formatted.replace(/[^0-9]/g, "");
}

function Step4Form({
  draft,
  onSave,
  onBack,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step4Payload) => void;
  onBack: () => void;
  saving: boolean;
}) {
  const existing = (draft.pricing ?? {}) as Partial<Step4Payload>;
  const [askingPrice, setAskingPrice] = useState(existing.asking_price?.toString() ?? "");
  const [negotiable, setNegotiable] = useState(existing.price_negotiable ?? false);
  const [maintenance, setMaintenance] = useState(existing.maintenance_monthly?.toString() ?? "");
  const [possession, setPossession] = useState(existing.possession_status ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const validate = useCallback((): boolean => {
    const errs: Record<string, string> = {};
    const price = parseInt(parsePriceInput(askingPrice));
    if (!askingPrice || isNaN(price)) {
      errs.asking_price = "Asking price is required";
    } else if (price < 100000) {
      errs.asking_price = "Minimum price is ₹1,00,000";
    }
    if (maintenance) {
      const m = parseInt(maintenance);
      if (isNaN(m) || m > 100000) errs.maintenance = "Maintenance must be ₹1,00,000 or less";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }, [askingPrice, maintenance]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const payload: Step4Payload = {
      asking_price: parseInt(parsePriceInput(askingPrice)),
    };
    if (negotiable) payload.price_negotiable = true;
    if (maintenance) payload.maintenance_monthly = parseInt(maintenance);
    if (possession) payload.possession_status = possession;
    onSave(payload);
  };

  const priceNum = parseInt(parsePriceInput(askingPrice));
  const priceLabel = !isNaN(priceNum) && priceNum >= 100000
    ? priceNum >= 10000000
      ? `₹${(priceNum / 10000000).toFixed(2)} Cr`
      : `₹${(priceNum / 100000).toFixed(2)} L`
    : null;

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Pricing
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Set your asking price. Transparent pricing builds buyer trust.
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>
          Asking Price (₹) <span style={{ color: "#c96b4f" }}>*</span>
        </label>
        <input
          type="text"
          value={formatPrice(askingPrice)}
          onChange={(e) => setAskingPrice(parsePriceInput(e.target.value))}
          placeholder="e.g. 85,00,000"
          style={inputStyle(!!errors.asking_price)}
        />
        <div style={{ display: "flex", justifyContent: "space-between", marginTop: "0.3rem" }}>
          {errors.asking_price ? (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: 0 }}>{errors.asking_price}</p>
          ) : (
            <span />
          )}
          {priceLabel && (
            <span style={{ fontSize: "0.8rem", color: "#888" }}>{priceLabel}</span>
          )}
        </div>
      </div>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={negotiable}
            onChange={(e) => setNegotiable(e.target.checked)}
            style={{ width: "18px", height: "18px", accentColor: "#c96b4f" }}
          />
          <span style={{ fontSize: "0.9rem", color: "#555" }}>Price is negotiable</span>
        </label>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1rem", marginBottom: "1.75rem" }}>
        <div>
          <label style={labelStyle}>Maintenance (₹/month)</label>
          <input
            type="number"
            value={maintenance}
            onChange={(e) => setMaintenance(e.target.value)}
            placeholder="e.g. 5000"
            style={inputStyle(!!errors.maintenance)}
          />
          {errors.maintenance && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.maintenance}</p>
          )}
        </div>
        <div>
          <label style={labelStyle}>Possession Status</label>
          <select
            value={possession}
            onChange={(e) => setPossession(e.target.value)}
            style={{ ...inputStyle(false), appearance: "auto" as const }}
          >
            {POSSESSION_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
        </div>
      </div>

      <div style={{ display: "flex", gap: "0.75rem" }}>
        <button
          type="button"
          onClick={onBack}
          style={{
            padding: "0.7rem 1.5rem",
            backgroundColor: "transparent",
            color: "#666",
            border: "1px solid rgba(0,0,0,0.12)",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Back
        </button>
        <button
          type="submit"
          disabled={saving}
          style={{
            padding: "0.7rem 1.5rem",
            backgroundColor: saving ? "#ccc" : "#c96b4f",
            color: "#fff",
            border: "none",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 600,
            cursor: saving ? "not-allowed" : "pointer",
            transition: "background-color 0.2s",
          }}
        >
          {saving ? "Saving..." : "Save & Continue"}
        </button>
      </div>
    </form>
  );
}

const DOCUMENT_ITEMS = [
  { key: "has_sale_deed", label: "Sale Deed" },
  { key: "has_khata", label: "Khata Certificate" },
  { key: "has_ec", label: "Encumbrance Certificate (EC)" },
  { key: "has_rera_registration", label: "RERA Registration" },
] as const;

function Step5Form({
  draft,
  onSave,
  onBack,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step5Payload) => void;
  onBack: () => void;
  saving: boolean;
}) {
  const existing = (draft.documents ?? {}) as Partial<Step5Payload>;
  const [docs, setDocs] = useState<Record<string, boolean>>({
    has_sale_deed: existing.has_sale_deed ?? false,
    has_khata: existing.has_khata ?? false,
    has_ec: existing.has_ec ?? false,
    has_rera_registration: existing.has_rera_registration ?? false,
  });
  const [reraNumber, setReraNumber] = useState(existing.rera_number ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const checkedCount = Object.values(docs).filter(Boolean).length;

  const validate = useCallback((): boolean => {
    const errs: Record<string, string> = {};
    if (reraNumber.trim().length > 50) {
      errs.rera_number = "RERA number must be 50 characters or fewer";
    }
    if (reraNumber.trim() && !/^[a-zA-Z0-9\-/]+$/.test(reraNumber.trim())) {
      errs.rera_number = "RERA number must be alphanumeric (hyphens and slashes allowed)";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }, [reraNumber]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const payload: Step5Payload = {};
    if (docs.has_sale_deed) payload.has_sale_deed = true;
    if (docs.has_khata) payload.has_khata = true;
    if (docs.has_ec) payload.has_ec = true;
    if (docs.has_rera_registration) payload.has_rera_registration = true;
    if (reraNumber.trim()) payload.rera_number = reraNumber.trim();
    onSave(payload);
  };

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Documents
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Which documents do you have ready? More documents = higher trust badge.
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        {DOCUMENT_ITEMS.map((item) => (
          <label
            key={item.key}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "0.6rem",
              padding: "0.7rem 0",
              borderBottom: "1px solid rgba(0,0,0,0.05)",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={docs[item.key] ?? false}
              onChange={(e) => setDocs({ ...docs, [item.key]: e.target.checked })}
              style={{ width: "18px", height: "18px", accentColor: "#c96b4f" }}
            />
            <span style={{ fontSize: "0.95rem", color: "#333" }}>{item.label}</span>
          </label>
        ))}
      </div>

      {checkedCount > 0 && (
        <div style={{
          padding: "0.6rem 0.9rem",
          background: "rgba(34,197,94,0.06)",
          border: "1px solid rgba(34,197,94,0.15)",
          borderRadius: "8px",
          fontSize: "0.85rem",
          color: "#16a34a",
          marginBottom: "1.25rem",
        }}>
          {checkedCount === 4
            ? "All documents ready — your listing will get a Verified badge!"
            : `${checkedCount} of 4 documents — more documents increase your trust score.`}
        </div>
      )}

      {docs.has_rera_registration && (
        <div style={{ marginBottom: "1.25rem" }}>
          <label style={labelStyle}>RERA Number</label>
          <input
            type="text"
            value={reraNumber}
            onChange={(e) => setReraNumber(e.target.value)}
            placeholder="e.g. PRM/KA/RERA/1251/446/PR/210930/003944"
            maxLength={50}
            style={inputStyle(!!errors.rera_number)}
          />
          {errors.rera_number && (
            <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.rera_number}</p>
          )}
        </div>
      )}

      <div style={{ display: "flex", gap: "0.75rem" }}>
        <button type="button" onClick={onBack} style={{ padding: "0.7rem 1.5rem", backgroundColor: "transparent", color: "#666", border: "1px solid rgba(0,0,0,0.12)", borderRadius: "8px", fontSize: "0.95rem", fontWeight: 500, cursor: "pointer" }}>
          Back
        </button>
        <button type="submit" disabled={saving} style={{ padding: "0.7rem 1.5rem", backgroundColor: saving ? "#ccc" : "#c96b4f", color: "#fff", border: "none", borderRadius: "8px", fontSize: "0.95rem", fontWeight: 600, cursor: saving ? "not-allowed" : "pointer", transition: "background-color 0.2s" }}>
          {saving ? "Saving..." : "Save & Continue"}
        </button>
      </div>
    </form>
  );
}

const PHOTO_COUNT_OPTIONS = [
  { value: "", label: "Select count" },
  { value: "0", label: "None yet" },
  { value: "3", label: "1-5 photos" },
  { value: "8", label: "6-10 photos" },
  { value: "15", label: "11-20 photos" },
];

function Step6Form({
  draft,
  onSave,
  onBack,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step6Payload) => void;
  onBack: () => void;
  saving: boolean;
}) {
  const existing = (draft.photos ?? {}) as Partial<Step6Payload>;
  const [photoCount, setPhotoCount] = useState(existing.photo_count?.toString() ?? "");
  const [hasFloorPlan, setHasFloorPlan] = useState(existing.has_floor_plan ?? false);
  const [videoUrl, setVideoUrl] = useState(existing.video_tour_url ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const validate = useCallback((): boolean => {
    const errs: Record<string, string> = {};
    if (videoUrl.trim().length > 500) {
      errs.video_url = "URL must be 500 characters or fewer";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }, [videoUrl]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const payload: Step6Payload = {};
    if (photoCount) payload.photo_count = parseInt(photoCount);
    if (hasFloorPlan) payload.has_floor_plan = true;
    if (videoUrl.trim()) payload.video_tour_url = videoUrl.trim();
    onSave(payload);
  };

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Photos & Media
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Properties with photos get 3x more interest. Photo upload is coming soon — for now, tell us what you have.
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>How many photos do you have?</label>
        <select
          value={photoCount}
          onChange={(e) => setPhotoCount(e.target.value)}
          style={{ ...inputStyle(false), appearance: "auto" as const }}
        >
          {PHOTO_COUNT_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </div>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={hasFloorPlan}
            onChange={(e) => setHasFloorPlan(e.target.checked)}
            style={{ width: "18px", height: "18px", accentColor: "#c96b4f" }}
          />
          <span style={{ fontSize: "0.9rem", color: "#555" }}>I have a floor plan</span>
        </label>
      </div>

      <div style={{ marginBottom: "1.75rem" }}>
        <label style={labelStyle}>
          Video Tour URL <span style={{ color: "#999", fontWeight: 400, fontSize: "0.8rem" }}>(optional)</span>
        </label>
        <input
          type="url"
          value={videoUrl}
          onChange={(e) => setVideoUrl(e.target.value)}
          placeholder="e.g. YouTube or Google Drive link"
          style={inputStyle(!!errors.video_url)}
        />
        {errors.video_url && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.video_url}</p>
        )}
      </div>

      <div style={{ display: "flex", gap: "0.75rem" }}>
        <button type="button" onClick={onBack} style={{ padding: "0.7rem 1.5rem", backgroundColor: "transparent", color: "#666", border: "1px solid rgba(0,0,0,0.12)", borderRadius: "8px", fontSize: "0.95rem", fontWeight: 500, cursor: "pointer" }}>
          Back
        </button>
        <button type="submit" disabled={saving} style={{ padding: "0.7rem 1.5rem", backgroundColor: saving ? "#ccc" : "#c96b4f", color: "#fff", border: "none", borderRadius: "8px", fontSize: "0.95rem", fontWeight: 600, cursor: saving ? "not-allowed" : "pointer", transition: "background-color 0.2s" }}>
          {saving ? "Saving..." : "Save & Continue"}
        </button>
      </div>
    </form>
  );
}

const AMENITY_OPTIONS = [
  "Swimming Pool", "Gym", "Clubhouse", "Park", "24/7 Security",
  "Power Backup", "Covered Parking", "Lift", "Children's Play Area",
  "Jogging Track", "Indoor Games", "EV Charging",
];

function Step7Form({
  draft,
  onSave,
  onBack,
  saving,
}: {
  draft: RegistrationDraft;
  onSave: (payload: Step7Payload) => void;
  onBack: () => void;
  saving: boolean;
}) {
  const existing = (draft.society_info ?? {}) as Partial<Step7Payload>;
  const [societyName, setSocietyName] = useState(existing.society_name ?? "");
  const [areaLocality, setAreaLocality] = useState(existing.area ?? "");
  const [totalUnits, setTotalUnits] = useState(existing.total_units?.toString() ?? "");
  const [amenities, setAmenities] = useState<Set<string>>(new Set(existing.amenities ?? []));
  const [notes, setNotes] = useState(existing.additional_notes ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const toggleAmenity = (amenity: string) => {
    const next = new Set(amenities);
    if (next.has(amenity)) next.delete(amenity);
    else next.add(amenity);
    setAmenities(next);
  };

  const validate = useCallback((): boolean => {
    const errs: Record<string, string> = {};
    if (societyName.trim().length > 100) {
      errs.society_name = "Society name must be 100 characters or fewer";
    }
    if (areaLocality.trim().length > 100) {
      errs.area = "Area must be 100 characters or fewer";
    }
    if (totalUnits) {
      const u = parseInt(totalUnits);
      if (isNaN(u) || u < 1 || u > 10000) errs.total_units = "Total units must be 1-10,000";
    }
    if (notes.length > 500) {
      errs.notes = "Notes must be 500 characters or fewer";
    }
    setErrors(errs);
    return Object.keys(errs).length === 0;
  }, [societyName, areaLocality, totalUnits, notes]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const payload: Step7Payload = {};
    if (societyName.trim()) payload.society_name = societyName.trim();
    if (areaLocality.trim()) payload.area = areaLocality.trim();
    if (totalUnits) payload.total_units = parseInt(totalUnits);
    if (amenities.size > 0) payload.amenities = Array.from(amenities);
    if (notes.trim()) payload.additional_notes = notes.trim();
    onSave(payload);
  };

  return (
    <form onSubmit={handleSubmit}>
      <h2 style={{ fontSize: "1.25rem", fontWeight: 700, marginBottom: "0.25rem", color: "#1a1a1a" }}>
        Society Information
      </h2>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
        Help buyers understand the community. All fields are optional.
      </p>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>Society Name</label>
        <input
          type="text"
          value={societyName}
          onChange={(e) => setSocietyName(e.target.value)}
          placeholder="e.g. Prestige Lakeside Habitat"
          maxLength={100}
          style={inputStyle(!!errors.society_name)}
        />
        {errors.society_name && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.society_name}</p>
        )}
      </div>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>Area / Locality</label>
        <input
          type="text"
          value={areaLocality}
          onChange={(e) => setAreaLocality(e.target.value)}
          placeholder="e.g. Whitefield, Sarjapur Road, Hebbal"
          maxLength={100}
          style={inputStyle(!!errors.area)}
        />
        {errors.area && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.area}</p>
        )}
      </div>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>Total Units in Society</label>
        <input
          type="number"
          value={totalUnits}
          onChange={(e) => setTotalUnits(e.target.value)}
          placeholder="e.g. 500"
          style={inputStyle(!!errors.total_units)}
        />
        {errors.total_units && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.total_units}</p>
        )}
      </div>

      <div style={{ marginBottom: "1.25rem" }}>
        <label style={labelStyle}>Amenities</label>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem" }}>
          {AMENITY_OPTIONS.map((a) => (
            <button
              key={a}
              type="button"
              onClick={() => toggleAmenity(a)}
              style={{
                padding: "0.4rem 0.8rem",
                border: amenities.has(a) ? "2px solid #c96b4f" : "1px solid rgba(0,0,0,0.12)",
                borderRadius: "20px",
                backgroundColor: amenities.has(a) ? "rgba(201,107,79,0.06)" : "#fff",
                color: amenities.has(a) ? "#c96b4f" : "#555",
                fontWeight: amenities.has(a) ? 600 : 400,
                fontSize: "0.85rem",
                cursor: "pointer",
                transition: "all 0.15s",
              }}
            >
              {a}
            </button>
          ))}
        </div>
      </div>

      <div style={{ marginBottom: "1.75rem" }}>
        <label style={labelStyle}>
          Additional Notes <span style={{ color: "#999", fontWeight: 400, fontSize: "0.8rem" }}>(optional)</span>
        </label>
        <textarea
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          placeholder="Anything else buyers should know about the society?"
          rows={3}
          maxLength={500}
          style={{
            ...inputStyle(!!errors.notes),
            resize: "vertical" as const,
            fontFamily: "inherit",
            lineHeight: 1.5,
          }}
        />
        {errors.notes && (
          <p style={{ color: "#ef4444", fontSize: "0.8rem", margin: "0.3rem 0 0" }}>{errors.notes}</p>
        )}
        <span style={{ fontSize: "0.78rem", color: notes.length > 450 ? "#f59e0b" : "#aaa", float: "right", marginTop: "0.2rem" }}>
          {notes.length}/500
        </span>
      </div>

      <div style={{ display: "flex", gap: "0.75rem" }}>
        <button type="button" onClick={onBack} style={{ padding: "0.7rem 1.5rem", backgroundColor: "transparent", color: "#666", border: "1px solid rgba(0,0,0,0.12)", borderRadius: "8px", fontSize: "0.95rem", fontWeight: 500, cursor: "pointer" }}>
          Back
        </button>
        <button type="submit" disabled={saving} style={{ padding: "0.7rem 1.5rem", backgroundColor: saving ? "#ccc" : "#c96b4f", color: "#fff", border: "none", borderRadius: "8px", fontSize: "0.95rem", fontWeight: 600, cursor: saving ? "not-allowed" : "pointer", transition: "background-color 0.2s" }}>
          {saving ? "Saving..." : "Complete Registration"}
        </button>
      </div>
    </form>
  );
}

function RegistrationComplete({ draft }: { draft: RegistrationDraft }) {
  const pct = draft.current_step >= 7 ? 100 : Math.round((draft.current_step / TOTAL_STEPS) * 100);
  const [publishState, setPublishState] = useState<"idle" | "publishing" | "success" | "error">("idle");
  const [publishResult, setPublishResult] = useState<PublishResult | null>(null);
  const [publishError, setPublishError] = useState("");

  const handlePublish = async () => {
    setPublishState("publishing");
    setPublishError("");
    try {
      const result = await publishRegistration(draft.id);
      setPublishResult(result);
      setPublishState("success");
      // Clear localStorage so a fresh visit starts a new registration
      localStorage.removeItem(DRAFT_ID_KEY);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Failed to publish listing";
      setPublishError(msg);
      setPublishState("error");
    }
  };

  // Published successfully — show success with link to dashboard
  if (publishState === "success" && publishResult) {
    return (
      <div style={{ textAlign: "center", padding: "2rem 0" }}>
        <div
          style={{
            width: "72px",
            height: "72px",
            margin: "0 auto 1.25rem",
            borderRadius: "50%",
            background: "rgba(34,197,94,0.08)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#16a34a" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        </div>
        <h2 style={{ fontSize: "1.4rem", fontWeight: 700, color: "#1a1a1a", marginBottom: "0.5rem" }}>
          Your Listing is Live!
        </h2>
        <p style={{ color: "#555", fontSize: "0.95rem", marginBottom: "0.5rem" }}>
          Seller ID: <strong>{publishResult.seller_id}</strong>
        </p>
        <p style={{ color: "#555", fontSize: "0.95rem", marginBottom: "0.5rem" }}>
          Property ID: <strong>{publishResult.property_id}</strong>
        </p>
        <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "2rem", maxWidth: "400px", margin: "0 auto 2rem" }}>
          Your property is now visible to buyers. Track interest and manage your listing from your dashboard.
        </p>
        <Link
          to={publishResult.dashboard_url}
          style={{
            display: "inline-block",
            padding: "0.7rem 1.5rem",
            backgroundColor: "#c96b4f",
            color: "#fff",
            border: "none",
            borderRadius: "8px",
            fontSize: "0.95rem",
            fontWeight: 600,
            textDecoration: "none",
            transition: "background-color 0.2s",
          }}
        >
          Go to Your Dashboard
        </Link>
      </div>
    );
  }

  // Default: show publish button (pre-publish state)
  return (
    <div style={{ textAlign: "center", padding: "2rem 0" }}>
      <div
        style={{
          width: "72px",
          height: "72px",
          margin: "0 auto 1.25rem",
          borderRadius: "50%",
          background: "rgba(34,197,94,0.08)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#16a34a" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      </div>
      <h2 style={{ fontSize: "1.4rem", fontWeight: 700, color: "#1a1a1a", marginBottom: "0.5rem" }}>
        Registration Complete!
      </h2>
      <p style={{ color: "#555", fontSize: "0.95rem", marginBottom: "0.25rem" }}>
        Your profile is {pct}% complete.
      </p>
      <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "2rem", maxWidth: "400px", margin: "0 auto 2rem" }}>
        Your listing is ready to go live. Click below to publish it and start receiving buyer interest.
      </p>

      {publishState === "error" && (
        <p style={{ color: "#dc2626", fontSize: "0.9rem", marginBottom: "1rem", maxWidth: "400px", margin: "0 auto 1rem" }}>
          {publishError}
        </p>
      )}

      <button
        onClick={handlePublish}
        disabled={publishState === "publishing"}
        style={{
          display: "inline-block",
          padding: "0.7rem 1.5rem",
          backgroundColor: publishState === "publishing" ? "#d4a090" : "#c96b4f",
          color: "#fff",
          border: "none",
          borderRadius: "8px",
          fontSize: "0.95rem",
          fontWeight: 600,
          cursor: publishState === "publishing" ? "not-allowed" : "pointer",
          transition: "background-color 0.2s",
          marginBottom: "1rem",
        }}
      >
        {publishState === "publishing" ? "Publishing..." : publishState === "error" ? "Retry Publish" : "Publish My Listing"}
      </button>

      <div style={{ marginTop: "0.5rem" }}>
        <Link
          to="/"
          style={{
            color: "#888",
            fontSize: "0.85rem",
            textDecoration: "none",
          }}
        >
          or go back to home
        </Link>
      </div>
    </div>
  );
}

export function SellerRegistrationPage() {
  const [draft, setDraft] = useState<RegistrationDraft | null>(null);
  const [viewStep, setViewStep] = useState(1);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [saving, setSaving] = useState(false);
  const [apiError, setApiError] = useState("");

  // On mount: check localStorage for existing draft, else create new
  useEffect(() => {
    const init = async () => {
      try {
        const existingId = localStorage.getItem(DRAFT_ID_KEY);
        if (existingId) {
          try {
            const existing = await getRegistration(existingId);
            setDraft(existing);
            // Resume from last completed step + 1; step 8 = completion screen
            const resumeStep = existing.current_step >= 7 ? 8 : existing.current_step + 1;
            setViewStep(resumeStep);
            setStatus("ready");
            return;
          } catch {
            // Draft not found on server, clear stale ID
            localStorage.removeItem(DRAFT_ID_KEY);
          }
        }
        // Create new draft
        const created = await createRegistration();
        localStorage.setItem(DRAFT_ID_KEY, created.id);
        const newDraft = await getRegistration(created.id);
        setDraft(newDraft);
        setViewStep(1);
        setStatus("ready");
      } catch (err) {
        console.error("Registration init failed:", err);
        setStatus("error");
      }
    };
    init();
  }, []);

  const handleStep1Save = async (payload: Step1Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 1, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(2);
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleStep2Save = async (payload: Step2Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 2, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(3);
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleStep3Save = async (payload: Step3Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 3, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(4);
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleStep4Save = async (payload: Step4Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 4, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(5);
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleStep5Save = async (payload: Step5Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 5, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(6);
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleStep6Save = async (payload: Step6Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 6, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(7);
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleStep7Save = async (payload: Step7Payload) => {
    if (!draft) return;
    setSaving(true);
    setApiError("");
    try {
      await updateRegistrationStep(draft.id, 7, payload);
      const updated = await getRegistration(draft.id);
      setDraft(updated);
      setViewStep(8); // 8 = completion screen
    } catch (err) {
      setApiError(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  if (status === "loading") {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", minHeight: "60vh" }}>
        <div style={{
          width: "32px",
          height: "32px",
          border: "3px solid rgba(201,107,79,0.15)",
          borderTopColor: "#c96b4f",
          borderRadius: "50%",
          animation: "spin 0.7s linear infinite",
        }} />
        <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      </div>
    );
  }

  if (status === "error" || !draft) {
    return (
      <div style={{ maxWidth: "600px", margin: "4rem auto", textAlign: "center", padding: "0 1rem" }}>
        <h2 style={{ color: "#1a1a1a", marginBottom: "0.5rem" }}>Something went wrong</h2>
        <p style={{ color: "#888" }}>Could not start registration. Please try again later.</p>
      </div>
    );
  }

  return (
    <>
      <Helmet>
        <title>Register as Seller | OpenEstates</title>
      </Helmet>

      <div style={{
        maxWidth: "600px",
        margin: "0 auto",
        padding: "2rem clamp(1rem, 4vw, 2rem)",
      }}>
        <h1 style={{
          fontSize: "1.5rem",
          fontWeight: 700,
          color: "#1a1a1a",
          marginBottom: "0.25rem",
        }}>
          Seller Registration
        </h1>
        <p style={{ color: "#888", fontSize: "0.9rem", marginBottom: "1.5rem" }}>
          List your property on OpenEstates in a few simple steps.
        </p>

        <ProgressBar currentStep={draft.current_step} />

        {apiError && (
          <div style={{
            padding: "0.7rem 1rem",
            background: "rgba(239,68,68,0.06)",
            border: "1px solid rgba(239,68,68,0.15)",
            borderRadius: "8px",
            color: "#dc2626",
            fontSize: "0.85rem",
            marginBottom: "1.25rem",
          }}>
            {apiError}
          </div>
        )}

        {viewStep === 1 && (
          <Step1Form draft={draft} onSave={handleStep1Save} saving={saving} />
        )}
        {viewStep === 2 && (
          <Step2Form
            draft={draft}
            onSave={handleStep2Save}
            onBack={() => setViewStep(1)}
            saving={saving}
          />
        )}
        {viewStep === 3 && (
          <Step3Form
            draft={draft}
            onSave={handleStep3Save}
            onBack={() => setViewStep(2)}
            saving={saving}
          />
        )}
        {viewStep === 4 && (
          <Step4Form
            draft={draft}
            onSave={handleStep4Save}
            onBack={() => setViewStep(3)}
            saving={saving}
          />
        )}
        {viewStep === 5 && (
          <Step5Form
            draft={draft}
            onSave={handleStep5Save}
            onBack={() => setViewStep(4)}
            saving={saving}
          />
        )}
        {viewStep === 6 && (
          <Step6Form
            draft={draft}
            onSave={handleStep6Save}
            onBack={() => setViewStep(5)}
            saving={saving}
          />
        )}
        {viewStep === 7 && (
          <Step7Form
            draft={draft}
            onSave={handleStep7Save}
            onBack={() => setViewStep(6)}
            saving={saving}
          />
        )}
        {viewStep >= 8 && (
          <RegistrationComplete draft={draft} />
        )}
      </div>
    </>
  );
}
