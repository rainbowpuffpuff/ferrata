#!/usr/bin/env bash
# The network session panel's headless test (qmltest/tst_network.qml: a stand-in ferrata_bridge_module behind the
# host's bridge; offscreen, software rendering) and the view's sealed channel under Node (tests/seal-test.mjs).
# Needs Qt 6's qmltestrunner with the QtQuick, Controls, Layouts and QtTest QML modules (QT_BIN), and Node.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
QT_BIN="${QT_BIN:-/usr/lib/qt6/bin}"
code=0
if [ -f tests/seal-test.mjs ] && command -v node >/dev/null 2>&1; then node tests/seal-test.mjs || code=1; else echo "tests/seal-test.mjs not run (no node, or no such test)"; fi
[ -x "$QT_BIN/qmltestrunner" ] || { echo "no $QT_BIN/qmltestrunner (QT_BIN)" >&2; exit 1; }
QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" QT_QUICK_BACKEND=software "$QT_BIN/qmltestrunner" -input qmltest/tst_network.qml || code=1
exit $code
