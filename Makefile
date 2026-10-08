# The application identifier, which is what a desktop names the program by:
# the desktop entry, the icon and the `app_id` the window carries are one name,
# or the desktop shows the program twice and with the wrong icon.
ID = ru.styxheim.zyterm

PREFIX ?= /usr/local
BINDIR = $(DESTDIR)$(PREFIX)/bin
APPDIR = $(DESTDIR)$(PREFIX)/share/applications
ICONDIR = $(DESTDIR)$(PREFIX)/share/icons/hicolor
# Where a file manager is told that this program opens a folder. KDE reads an
# entry of its own and puts the item straight in the context menu; GNOME Files
# has no such file and takes an extension instead.
KIODIR = $(DESTDIR)$(PREFIX)/share/kio/servicemenus
NAUTILUSDIR = $(DESTDIR)$(PREFIX)/share/nautilus-python/extensions
OSCDIR = $(DESTDIR)$(PREFIX)/share/zyterm/osc
SCRIPTDIR = $(DESTDIR)$(PREFIX)/share/zyterm/scripts

# The scripts a transfer is, and the shell they send. They are read at run
# time and not built in, so they are installed beside the program and found
# there: `scripts/` under the shared directory of the application is the third
# of the four places a script may stand.
SCRIPTS = $(shell find scripts -type f \( -name '*.lua' -o -name '*.sh' -o -name 'Manifest.yaml' \))

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
	cargo build --release -p zyt-script

build-pgo:
	cargo pgo test -- --workspace --no-fail-fast || true
	cargo pgo optimize build -- --bin zyterm
	cargo pgo optimize build -- -p zyt-script --bin zyt-script

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
	sudo install -Dm755 $(FROM)/zyt-script $(BINDIR)/zyt-script
	sudo install -Dm644 assets/$(ID).desktop $(APPDIR)/$(ID).desktop
	sudo install -Dm644 assets/$(ID).open.desktop $(KIODIR)/$(ID).open.desktop
	sudo install -Dm644 assets/nautilus/zyterm.py $(NAUTILUSDIR)/zyterm.py
	sudo install -Dm644 assets/icon.svg $(ICONDIR)/scalable/apps/$(ID).svg
	sudo install -Dm644 assets/icon.png $(ICONDIR)/256x256/apps/$(ID).png
	sudo install -Dm644 assets/osc/osc133-bash.sh $(OSCDIR)/osc133-bash.sh
	for file in $(SCRIPTS); do \
		sudo install -Dm644 $$file $(SCRIPTDIR)/$${file#scripts/}; \
	done
	sudo update-desktop-database $(APPDIR) 2>/dev/null || true

uninstall:
	sudo rm -f $(BINDIR)/zyterm
	sudo rm -f $(BINDIR)/zyt-script
	sudo rm -f $(APPDIR)/$(ID).desktop
	sudo rm -f $(KIODIR)/$(ID).open.desktop
	sudo rm -f $(NAUTILUSDIR)/zyterm.py
	sudo rm -f $(ICONDIR)/scalable/apps/$(ID).svg
	sudo rm -f $(ICONDIR)/256x256/apps/$(ID).png
	sudo rm -f $(OSCDIR)/osc133-bash.sh
	for file in $(SCRIPTS); do \
		sudo rm -f $(SCRIPTDIR)/$${file#scripts/}; \
	done
	sudo rmdir -p --ignore-fail-on-non-empty $(OSCDIR) 2>/dev/null || true
	find $(SCRIPTDIR) -depth -type d -exec sudo rmdir --ignore-fail-on-non-empty {} + 2>/dev/null || true
	sudo update-desktop-database $(APPDIR) 2>/dev/null || true
