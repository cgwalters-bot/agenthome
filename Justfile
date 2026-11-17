default:
    @just --list

# Offline configuration lint and positive/negative fixtures; Python stdlib only.
check-config:
    python3 tests/check_config.py

# Offline discovery regression tests, including recorded provider/broker fixtures.
check-models:
    python3 -B -m unittest discover -s tests -p 'test_models.py'

# Opt-in discovery using a broker pair from the environment or deployed home config.
# May send up to eight minimal inference probes when /models is unavailable.
discover-models *args:
    python3 -B tools/discover_models.py {{args}}

check: check-config check-models
