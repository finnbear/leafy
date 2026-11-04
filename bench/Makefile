# The benchmark suite. `make all` runs everything (under an hour); each
# benchmark also runs on its own:
#
#   make apps        size and runtime speed of the apps: frontier.svg
#   make buildtime   build time against size, 1 to 100 components: buildtime.svg
#   make ssr         server-rendering time: ssr.svg
#   make charts      redraw the charts from the last results (in out/)
#
# Results go to out/. RUNS sets speed.mjs's runs per build.

RUNS ?= 10
APPS := todomvc todomvc_struct jsfb jsfb_struct life
LIBS := yew leafy
PROFILES := speed size
TOOLS := trunk wasm-tools brotli node python3 bc taskset cargo
CHROME ?= chromium
export CHROME

# One at a time: timings are only fair on an otherwise idle machine.
.NOTPARALLEL:
.PHONY: all apps sizes speed buildtime ssr charts tools clean

all: apps buildtime ssr

# Checks the tools the benchmarks need are installed.
tools:
	@for t in $(TOOLS) $(CHROME); do command -v $$t > /dev/null || { echo "missing: $$t" >&2; exit 1; }; done

# Each app with each library and profile.
sizes: tools
	@for app in $(APPS); do for lib in $(LIBS); do for p in $(PROFILES); do ./build.sh $$app $$lib $$p || exit 1; done; done; done

# Runtime speed of those builds.
speed: sizes
	@for app in $(APPS); do for p in $(PROFILES); do node speed.mjs $$app $(RUNS) $$p || exit 1; done; done

apps: speed
	node plot.mjs

buildtime: tools
	./buildtime.sh 3
	node buildtime.mjs

ssr: tools
	./ssr.sh 30
	node ssr.mjs

charts:
	node plot.mjs
	node buildtime.mjs
	node ssr.mjs

clean:
	rm -rf out
