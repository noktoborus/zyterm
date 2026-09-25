# The application identifier, which is what a desktop names the program by:
# the desktop entry, the icon and the `app_id` the window carries are one name,
# or the desktop shows the program twice and with the wrong icon.
ID = ru.styxheim.zyterm

PREFIX ?= /usr/local
BINDIR = $(DESTDIR)$(PREFIX)/bin
APPDIR = $(DESTDIR)$(PREFIX)/share/applications
ICONDIR = $(DESTDIR)$(PREFIX)/share/icons/hicolor
OSCDIR = $(DESTDIR)$(PREFIX)/share/zyterm/osc

# There are two builds and an install for each, so which one goes on the
# machine is said out loud rather than decided by whichever ran last.
#
# cargo-pgo names the target triple itself, which is what keeps the profiling
# flags off the build scripts, so `build-pgo` lands under that triple while
# `build` writes to target/release. The two never tread on one another.
HOST = $(shell rustc -vV | sed -n 's/^host: //p')
DIR = target/release
PGODIR = target/$(HOST)/release

.PHONY: build build-pgo install install-pgo uninstall

build:
	cargo build --release
	cargo build --release -p sh-xfer

build-pgo:
	cargo pgo test -- --workspace --no-fail-fast || true
	cargo pgo optimize build -- --bin zyterm
	cargo pgo optimize build -- -p sh-xfer --bin sh-xfer

# One body, two sources. `install` takes what `build` left in target/release
# and `install-pgo` what `build-pgo` left under the target triple, so which
# binary goes on the machine is named rather than decided by whichever build
# ran last. Neither of them builds anything, and each says which build it
# wants when that build is not there.
install:     FROM = $(DIR)
install:     WANT = build
install-pgo: FROM = $(PGODIR)
install-pgo: WANT = build-pgo

install install-pgo:
	@test -x $(FROM)/zyterm || { echo "run 'make $(WANT)' first: the binaries are taken from $(FROM)"; exit 1; }
	sudo install -Dm755 $(FROM)/zyterm $(BINDIR)/zyterm
	sudo install -Dm755 $(FROM)/sh-xfer $(BINDIR)/sh-xfer
	sudo install -Dm644 assets/$(ID).desktop $(APPDIR)/$(ID).desktop
	sudo install -Dm644 assets/icon.svg $(ICONDIR)/scalable/apps/$(ID).svg
	sudo install -Dm644 assets/icon.png $(ICONDIR)/256x256/apps/$(ID).png
	sudo install -Dm644 assets/osc/osc133-bash.sh $(OSCDIR)/osc133-bash.sh

uninstall:
	sudo rm -f $(BINDIR)/zyterm
	sudo rm -f $(BINDIR)/sh-xfer
	sudo rm -f $(APPDIR)/$(ID).desktop
	sudo rm -f $(ICONDIR)/scalable/apps/$(ID).svg
	sudo rm -f $(ICONDIR)/256x256/apps/$(ID).png
	sudo rm -f $(OSCDIR)/osc133-bash.sh
	sudo rmdir -p --ignore-fail-on-non-empty $(OSCDIR) 2>/dev/null || true
