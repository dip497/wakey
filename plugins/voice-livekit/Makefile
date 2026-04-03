# Wakey Voice Plugin - Development Commands

.PHONY: install test run clean

install:
	pip install -r requirements.txt

test:
	python3 test_protocol.py

run:
	python3 main.py

clean:
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true

# Run with test environment
run-test:
	@echo "Testing plugin startup (will fail without valid credentials)..."
	python3 -c "import main; print('Plugin imports OK')"