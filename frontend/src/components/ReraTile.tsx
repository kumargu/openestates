import type { ReraInfo } from "../lib/types";

function formatIndianCurrency(value: number): string {
  if (value >= 1_00_00_000) return `\u20B9${(value / 1_00_00_000).toFixed(1)} Cr`;
  if (value >= 1_00_000) return `\u20B9${(value / 1_00_000).toFixed(1)} L`;
  return `\u20B9${value.toLocaleString("en-IN")}`;
}

function formatDate(dateStr: string): string {
  const separators = /[/\-]/;
  const parts = dateStr.split(separators);
  if (parts.length === 3) {
    const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    // YYYY-MM-DD
    if (parts[0].length === 4) {
      return `${months[parseInt(parts[1], 10) - 1]} ${parts[0]}`;
    }
    // DD/MM/YYYY
    const month = parseInt(parts[1], 10);
    if (month >= 1 && month <= 12) {
      return `${months[month - 1]} ${parts[2]}`;
    }
  }
  return dateStr;
}

function StatCard({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div style={{
      padding: "0.6rem 0.75rem",
      borderRadius: "var(--radius-sm)",
      backgroundColor: "var(--color-bg-elevated)",
      border: "1px solid var(--color-border)",
    }}>
      <div style={{
        fontSize: "0.62rem",
        fontWeight: 600,
        textTransform: "uppercase",
        letterSpacing: "0.06em",
        color: "var(--color-text-muted)",
        marginBottom: "0.2rem",
      }}>
        {label}
      </div>
      <div style={{
        fontSize: accent ? "0.95rem" : "0.85rem",
        fontWeight: 600,
        color: "var(--color-text)",
      }}>
        {value}
      </div>
    </div>
  );
}

export function ReraTile({ rera }: { rera: ReraInfo }) {
  // Not found on RERA portal — compact red flag
  if (!rera.registered) {
    return (
      <div className="section-card" data-testid="rera-not-found-tile">
        <div style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}>
          <div className="section-card-header" style={{ marginBottom: 0 }}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
            <h2>RERA Verification</h2>
          </div>
          <span style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "0.3rem",
            fontSize: "0.75rem",
            fontWeight: 600,
            padding: "0.2rem 0.65rem",
            borderRadius: "var(--radius-xl)",
            backgroundColor: "#fef2f2",
            color: "#dc2626",
            border: "1px solid #fecaca",
          }}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
            Not Found
          </span>
        </div>
        <p style={{
          margin: "0.75rem 0 0",
          fontSize: "0.82rem",
          color: "var(--color-text-secondary)",
          lineHeight: 1.6,
        }}>
          Not found on the Karnataka RERA portal. This may mean the project is registered under a different name, or is not RERA-registered.
        </p>
      </div>
    );
  }

  const landPct = (rera.total_project_cost_inr && rera.land_cost_inr)
    ? Math.round((rera.land_cost_inr / rera.total_project_cost_inr) * 100)
    : null;
  const buildPct = (rera.total_project_cost_inr && rera.construction_cost_inr)
    ? Math.round((rera.construction_cost_inr / rera.total_project_cost_inr) * 100)
    : null;

  return (
    <div className="section-card" data-testid="rera-tile">
      {/* Header + badge */}
      <div style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "flex-start",
        marginBottom: "0.5rem",
      }}>
        <div>
          <div className="section-card-header" style={{ marginBottom: "0.25rem" }}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
            <h2>RERA Verification</h2>
          </div>
          {/* Registration number as subtitle — it's long, deserves its own line */}
          {rera.registration_number && (
            <div style={{
              fontSize: "0.78rem",
              color: "var(--color-text-secondary)",
              fontFamily: "var(--font-mono, monospace)",
              letterSpacing: "0.01em",
              marginLeft: "1.5rem",
            }}>
              {rera.registration_number}
            </div>
          )}
        </div>
        <span style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "0.3rem",
          fontSize: "0.75rem",
          fontWeight: 600,
          padding: "0.2rem 0.65rem",
          borderRadius: "var(--radius-xl)",
          backgroundColor: "var(--color-positive-bg)",
          color: "var(--color-positive)",
          border: "1px solid var(--color-positive-border)",
          flexShrink: 0,
        }}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="20 6 9 17 4 12" />
          </svg>
          Verified
        </span>
      </div>

      {/* Delay warning */}
      {rera.delay_months != null && rera.delay_months > 0 && (
        <div style={{
          display: "flex",
          alignItems: "center",
          gap: "0.5rem",
          padding: "0.5rem 0.75rem",
          borderRadius: "var(--radius-sm)",
          backgroundColor: "#fdf8f0",
          border: "1px solid #e8dcc0",
          marginBottom: "0.75rem",
          fontSize: "0.8rem",
          color: "var(--color-warning)",
          fontWeight: 500,
        }}>
          {"\u26A0"} {rera.delay_months} months behind original schedule
        </div>
      )}

      {/* Key stats — compact grid */}
      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))",
        gap: "0.5rem",
        marginBottom: "0.75rem",
      }}>
        {rera.status && <StatCard label="Status" value={rera.status} accent />}
        {rera.completion_date && <StatCard label="Completion" value={formatDate(rera.completion_date)} accent />}
        {rera.total_units != null && <StatCard label="Units" value={String(rera.total_units)} />}
        {rera.complaints_count != null && (
          <StatCard
            label="Complaints"
            value={
              rera.complaints_resolved_pct != null
                ? `${rera.complaints_count} (${rera.complaints_resolved_pct}% resolved)`
                : String(rera.complaints_count)
            }
          />
        )}
      </div>

      {/* Cost transparency — compact inline */}
      {(rera.land_cost_inr != null || rera.construction_cost_inr != null) && (
        <div style={{
          padding: "0.75rem",
          borderRadius: "var(--radius-sm)",
          backgroundColor: "var(--color-bg-elevated)",
          border: "1px solid var(--color-border)",
          marginBottom: "0.75rem",
        }}>
          <div style={{
            fontSize: "0.62rem",
            fontWeight: 600,
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            color: "var(--color-text-muted)",
            marginBottom: "0.5rem",
          }}>
            Cost Breakdown (RERA Filing)
          </div>
          <div style={{
            display: "flex",
            gap: "1.25rem",
            flexWrap: "wrap",
            alignItems: "baseline",
          }}>
            {rera.land_cost_inr != null && (
              <div>
                <span style={{ fontSize: "0.92rem", fontWeight: 600 }}>{formatIndianCurrency(rera.land_cost_inr)}</span>
                <span style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", marginLeft: "0.3rem" }}>
                  land{landPct != null ? ` (${landPct}%)` : ""}
                </span>
              </div>
            )}
            {rera.construction_cost_inr != null && (
              <div>
                <span style={{ fontSize: "0.92rem", fontWeight: 600 }}>{formatIndianCurrency(rera.construction_cost_inr)}</span>
                <span style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", marginLeft: "0.3rem" }}>
                  construction{buildPct != null ? ` (${buildPct}%)` : ""}
                </span>
              </div>
            )}
            {rera.total_project_cost_inr != null && (
              <div>
                <span style={{ fontSize: "0.92rem", fontWeight: 700 }}>{formatIndianCurrency(rera.total_project_cost_inr)}</span>
                <span style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", marginLeft: "0.3rem" }}>total</span>
              </div>
            )}
            {rera.cost_per_unit_inr != null && (
              <div>
                <span style={{ fontSize: "0.92rem", fontWeight: 600 }}>{formatIndianCurrency(rera.cost_per_unit_inr)}</span>
                <span style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", marginLeft: "0.3rem" }}>per unit</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Builder + financials — compact inline items */}
      <div style={{
        display: "flex",
        flexWrap: "wrap",
        gap: "0.5rem 1.25rem",
        fontSize: "0.82rem",
        color: "var(--color-text-secondary)",
        lineHeight: 1.7,
        marginBottom: "0.75rem",
      }}>
        {rera.builder_total_projects != null && (
          <span>
            {rera.builder_total_projects} RERA projects
            {rera.builder_states && rera.builder_states.length > 0
              ? ` across ${rera.builder_states.length} states`
              : ""}
          </span>
        )}
        {rera.builder_revocations != null && (
          <span style={{ color: rera.builder_revocations > 0 ? "var(--color-warning)" : undefined }}>
            {rera.builder_revocations} revocation{rera.builder_revocations !== 1 ? "s" : ""}
          </span>
        )}
        {rera.land_litigation != null && (
          <span style={{ color: rera.land_litigation ? "var(--color-negative)" : "var(--color-positive)" }}>
            {rera.land_litigation ? "Land litigation reported" : "No litigation"}
          </span>
        )}
        {rera.escrow_bank && <span>Escrow: {rera.escrow_bank}</span>}
        {rera.has_borrowing === false && <span>No borrowing</span>}
        {rera.has_borrowing === true && <span style={{ color: "var(--color-warning)" }}>Has borrowing</span>}
        {rera.has_mortgage === false && <span>No mortgage</span>}
        {rera.has_mortgage === true && <span style={{ color: "var(--color-warning)" }}>Has mortgage</span>}
      </div>

      {/* Footer */}
      <div style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        paddingTop: "0.5rem",
        borderTop: "1px solid var(--color-border)",
      }}>
        {rera.last_verified && (
          <span style={{ fontSize: "0.7rem", color: "var(--color-text-muted)" }}>
            Last verified: {formatDate(rera.last_verified)}
          </span>
        )}
        {rera.rera_portal_url && (
          <a
            href={rera.rera_portal_url}
            target="_blank"
            rel="noopener noreferrer"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "0.3rem",
              fontSize: "0.75rem",
              fontWeight: 500,
              color: "var(--color-accent)",
              textDecoration: "none",
            }}
          >
            View on RERA Portal
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
              <polyline points="15 3 21 3 21 9" />
              <line x1="10" y1="14" x2="21" y2="3" />
            </svg>
          </a>
        )}
      </div>
    </div>
  );
}

export function ReraPendingTile() {
  return (
    <div className="section-card" data-testid="rera-pending-tile" style={{ opacity: 0.7 }}>
      <div className="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        </svg>
        <h2>RERA Verification</h2>
      </div>
      <p style={{
        margin: 0,
        fontSize: "0.85rem",
        color: "var(--color-text-muted)",
        lineHeight: 1.6,
      }}>
        RERA verification in progress &mdash; data will appear once verified.
      </p>
    </div>
  );
}
