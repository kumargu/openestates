#!/usr/bin/env python3
"""Compatibility entry point for the broader DAG convergence audit.

The old search-only command now delegates to `audit_dag_convergence.py`, which
scans search vocabulary, source labels, map layers, recommendation branches,
evidence sections, Area Tracker terms, and warning/red-flag terms.
"""

from __future__ import annotations

import runpy
from pathlib import Path


if __name__ == "__main__":
    audit_path = Path(__file__).with_name("audit_dag_convergence.py")
    runpy.run_path(str(audit_path), run_name="__main__")
