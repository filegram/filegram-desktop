VERSION ?= 0.2.12

srpm:
	@set -e; \
	echo '$(VERSION)' | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$$' || { echo 'VERSION must be X.Y.Z' >&2; exit 1; }; \
	work=$$(mktemp -d); \
	trap 'rm -rf "$$work"' EXIT; \
	dnf -y install rpm-build rpmdevtools; \
	sed 's/__VERSION__/$(VERSION)/g' '$(spec)' > "$$work/filegram.spec"; \
	spectool -g -C "$$work" "$$work/filegram.spec"; \
	rpmbuild -bs "$$work/filegram.spec" \
		--define "_sourcedir $$work" \
		--define "_srcrpmdir $(outdir)"
