default:
    @just --list

# Offline configuration lint and positive/negative fixtures; Python stdlib only.
check-config:
    python3 tests/check_config.py

check: check-config
