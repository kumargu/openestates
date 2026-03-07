# OpenEstates Matching Engine Prototype
## Day 1 – Project Skeleton and Terminal App Foundation

### Overview

This project is an experimental prototype for **OpenEstates**, a system designed to improve how buyers and sellers of residential real estate are matched.

Traditional real estate platforms rely mostly on static filters such as price, location, and BHK. Brokers fill the gap by understanding deeper context such as buyer flexibility, seller urgency, negotiation style, and emotional constraints.

The goal of this prototype is to explore whether a **context-driven matching engine** can outperform simple filter-based search by modeling:

- buyer preferences and flexibility
- seller urgency and constraints
- property characteristics
- evolving user context over time
- signals extracted from conversations

We are **not building a full product**.  
This prototype is a **simulation environment** that allows us to test the quality of a matching engine.

The system will eventually include:

- synthetic buyers, sellers, and property listings
- a context graph representing user preferences
- a matching engine that ranks compatibility
- simulated conversations with an AI coach
- learning loops based on simulated outcomes

The interface for this prototype will be a **Terminal User Interface (TUI)**.

The system will run locally on a developer laptop.

We will optionally integrate **ZeroClaw** later to support agent-like behavior such as context extraction and match explanation.

---

# Development Process

This project is organized into **daily build tasks**.

Each day focuses on a specific subsystem of the prototype.  
Future days will include:

- synthetic market generation
- context graph modeling
- baseline search engine
- matching engine
- simulated conversations
- AI coach
- outcome simulation

For now, focus **only on Day 1**.

We will provide additional day instructions later.

---

# Day 1 Goal

Create the **initial project skeleton and a runnable terminal application**.

At the end of Day 1 we want:

- a working CLI/TUI application
- a clean repository structure
- configuration files
- placeholder modules for future components
- a command that launches the terminal interface

The application does **not need real functionality yet**.

It should simply provide a structured foundation for the future matching engine.

---

# Technology Choices

Use the following stack unless there is a strong reason not to.

Language:
Python 3.11+

Terminal UI:
`textual` (preferred) or `rich`

Data storage (initial):
JSON files

Later we may add:
SQLite or Postgres

Do not add heavy frameworks.

---

# Project Structure

Create the following repository layout.

openestates/
│
├── app/
│ ├── main.py
│ └── tui.py
│
├── engine/
│ ├── match_engine.py
│ └── scoring.py
│
├── graph/
│ ├── graph_store.py
│ └── context_graph.py
│
├── agents/
│ ├── coach.py
│ └── signal_extractor.py
│
├── simulation/
│ ├── market_generator.py
│ └── conversation_simulator.py
│
├── data/
│ └── sample_market.json
│
├── days/
│ └── day01.md
│
├── requirements.txt
└── README.md


Many of these files will contain **placeholders** today.

---

# Terminal Interface (Minimal Version)

The TUI should support a simple navigation screen.

Example:


OpenEstates Prototype

Load synthetic market

Show buyer

Show seller

Run matching engine

Exit


These options do not need to work yet.

They can simply print:


Feature not implemented yet


The purpose is to confirm the app structure works.

---

# Files to Implement Today

### main.py

Entry point for the application.

Responsibilities:

- start the terminal UI
- initialize configuration
- load environment settings

---

### tui.py

Contains the terminal interface.

Should:

- render the main menu
- accept simple input
- call placeholder functions

---

### README.md

Write a short README explaining:

- the goal of the prototype
- how to run the project
- that the system is experimental

---

### requirements.txt

Include only minimal dependencies.

Example:


textual
rich


---

# Important Constraints

Do NOT implement:

- matching logic
- synthetic data generator
- context graph
- conversation system
- ZeroClaw integration

Those belong to later days.

Today is only **project foundation**.

---

# Running the App

After implementation, the following command should work:


python app/main.py


The terminal interface should launch successfully.

---

# Manual Verification Checklist

Before finishing Day 1 confirm:

- The project runs with `python app/main.py`
- The TUI menu appears
- Menu items respond to input
- Placeholder modules exist for future work
- Repository structure matches the specification
- README explains the project

---

# Deliverable

A runnable terminal prototype with clean architecture that future days can extend.

Do not implement extra features beyond this scope.

Keep the code simple, readable, and modular.
