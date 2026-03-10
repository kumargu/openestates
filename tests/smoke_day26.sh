#!/bin/bash
set -e

echo "=== Day 26 Smoke Tests ==="

echo "1. Entity Resolver"
python3 -m pipeline.entity_resolver > /dev/null 2>&1
echo "   PASS"

echo "2. Orchestrator Help"
python3 -m pipeline.orchestrate --help > /dev/null
echo "   PASS"

echo "3. Orchestrator List Stale"
python3 -m pipeline.orchestrate --list-stale > /dev/null
echo "   PASS"

echo "4. RERA Skill Loads"
python3 -c "from pipeline.skills.fetch_rera import FetchReraSkill; print('OK')"
echo "   PASS"

echo "5. Area Skill Loads (v2)"
python3 -c "from pipeline.skills.learn_area import LearnAreaSkill; s=LearnAreaSkill(); assert s.version=='2.0'"
echo "   PASS"

echo "6. verify_rera.py Deleted"
python3 -c "
try:
    from pipeline.skills.verify_rera import VerifyReraSkill
    exit(1)
except (ModuleNotFoundError, ImportError):
    print('OK - correctly deleted')
"
echo "   PASS"

echo "7. Entity Resolver Self-Test"
python3 -c "
from pipeline.entity_resolver import EntityResolver
r = EntityResolver()
assert r.resolve_society('Prestige Lakeside Habitat') == 'society:prestige-lakeside-habitat'
assert r.resolve_area('Whitefield') == 'area:whitefield'
assert r.resolve_builder('M/s. SOBHA LIMITED').startswith('builder:')
print('OK')
"
echo "   PASS"

echo "8. RERA Fact Production"
python3 -c "
from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
detail = ReraProjectDetail(
    registration_number='TEST', status='APPROVED',
    total_project_cost_inr=100000000.0, complaints_count=5,
)
facts = rera_detail_to_facts(detail)
assert len(facts) > 5, f'Expected >5 facts, got {len(facts)}'
assert all(f.confidence == 1.0 for f in facts), 'All RERA facts should be confidence 1.0'
assert all(f.source.source_type == 'Rera' for f in facts), 'All sources should be Rera'
print(f'OK - {len(facts)} facts produced')
"
echo "   PASS"

echo "9. Freshness Tracker"
python3 -c "
from pipeline.entity_resolver import FreshnessTracker
t = FreshnessTracker(path='/tmp/smoke_freshness.json')
assert t.is_stale('society:test', 'rera'), 'New entity should be stale'
t.mark_fresh('society:test', 'rera')
assert not t.is_stale('society:test', 'rera', ttl_days=1), 'Just-marked should be fresh'
assert t.is_stale('society:test', 'rera', ttl_days=0), 'TTL=0 should be stale'
print('OK')
"
echo "   PASS"

echo "10. Unit Tests"
python3 -m tests.test_pipeline_day26 > /dev/null 2>&1
echo "   PASS"

echo ""
echo "=== All Day 26 smoke tests passed ==="
