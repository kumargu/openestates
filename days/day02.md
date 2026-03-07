# OpenEstates Matching Engine Prototype
## Day 2 – Synthetic Market Schema and Generator

Before starting today's work, read the following files:

- CLAUDE.md
- LEARNING.md
- days/day01.md

These documents define the architectural philosophy, learning model, and coding expectations.

Today we begin building the **synthetic world** that OpenEstates will operate in.

This synthetic market will allow us to test whether contextual matching can outperform simple filter-based search.

We are not using real property data.  
Instead we will generate **synthetic buyers, sellers, and property listings**.

The system must later be able to simulate:

- buyer preferences
- seller urgency
- property attributes
- hidden compatibility signals
- eventual deal outcomes

This simulation environment is critical because it allows us to evaluate the matching engine objectively.

---

# Day 2 Goal

Build a **synthetic market generator** that produces buyers, sellers, and property listings with realistic attributes.

The generator should produce a dataset that can be stored as JSON.

This dataset will later be used by:

- the matching engine
- the baseline search system
- the evaluation system
- simulated conversations

The generator must produce **reproducible results**.

---

# Entities

The synthetic market must include three core entities:

Buyer  
Seller  
Property

Each entity should have both **visible attributes** and **hidden attributes**.

Hidden attributes are used by the simulator later to determine the "true compatibility" of matches.

---

# Property Schema

Each property should contain fields like:

id  
area (Whitefield, Sarjapur, HSR, etc.)  
price  
bhk  
floor  
facing  
metro_distance_minutes  
builder_quality_score  
society_quality_score  
litigation_risk  
maintenance_cost  
possession_status (ready / under_construction)

Also include:

document_completeness_score  
sunlight_score  
noise_score

These help later with contextual matching.

---

# Seller Schema

Seller objects represent property owners.

Fields:

id  
property_id  
urgency_level  
visit_tolerance  
negotiation_style  
price_flexibility  
possession_flexibility  
privacy_preference

Hidden attributes may include:

true_price_floor  
true_urgency  
friction_sensitivity

---

# Buyer Schema

Buyer objects represent potential purchasers.

Fields:

id  
budget_min  
budget_max  
preferred_areas  
preferred_bhk  
timeline_months  
metro_preference_weight  
society_quality_weight  
document_safety_weight  
renovation_tolerance

Hidden attributes may include:

true_budget_limit  
hidden_area_flexibility  
negotiation_patience  
risk_tolerance

These hidden attributes will later help determine the true compatibility of matches.

---

# Market Generator

Create a module:

simulation/market_generator.py

The generator should produce:

- N buyers
- N sellers
- N properties

Properties should be randomly distributed across a small set of Bengaluru areas.

Example areas:

Whitefield  
Sarjapur  
HSR  
Bellandur  
Electronic City

Price ranges should loosely resemble Bengaluru apartment markets.

For example:

80L – 3Cr range.

Use randomness but allow seeding for reproducibility.

Example usage:


python app/main.py generate_market


This should produce a JSON file:


data/synthetic_market.json


---

# JSON Output Structure

Example structure:


{
"buyers": [...],
"sellers": [...],
"properties": [...]
}


Do not overcomplicate this format.

---

# TUI Integration

Extend the terminal interface with a new option:


Generate synthetic market


When selected, the system should:

- generate the market
- save it to JSON
- print summary statistics

Example output:


Generated synthetic market
Properties: 200
Buyers: 150
Sellers: 200


---

# Important Constraints

Do NOT implement:

- matching engine
- context graph
- conversations
- AI coach
- ZeroClaw integration
- outcome simulation

Those belong to later days.

Focus only on **synthetic market generation**.

---

# Deliverables

By the end of Day 2 we should have:

- a working market generator
- JSON output file
- new TUI command
- clean schemas for buyers, sellers, properties

---

# Manual Verification

After implementing:

Run:


python app/main.py


Choose:


Generate synthetic market


Confirm:

- JSON file is produced
- entities are valid
- counts match expectations
- fields look reasonable

---

# Future Context

Later days will add:

- baseline search
- contextual matching
- conversation simulation
- AI coach
- evaluation against hidden compatibility

For now the goal is simply to **create a realistic synthetic market environment**.

Keep the code simple, readable, and deterministic.