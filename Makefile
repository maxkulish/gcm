.PHONY: help pi-init

help:
	@echo "gcm Makefile"
	@echo ""
	@echo "Pi:"
	@echo "  make pi-init  - Install npm deps for all .pi/extensions/*"

pi-init:
	@for ext in .pi/extensions/*/package.json; do \
		[ -f "$$ext" ] || continue; \
		dir=$$(dirname "$$ext"); \
		echo "Installing deps in $$dir..."; \
		if [ -f "$$dir/package-lock.json" ]; then \
			(cd "$$dir" && npm ci --silent); \
		else \
			(cd "$$dir" && npm install --silent); \
		fi; \
	done
	@echo "Pi extensions ready"
