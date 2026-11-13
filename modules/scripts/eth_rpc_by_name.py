"""Make logos-evm-eth-rpc-module @ 6965f32 build with logos-module-builder 5f74cdb (our modules' pin).

    python3 -I eth_rpc_by_name.py SOURCE_DIR

Built with that builder, the module compiles and links, then its install fails while normalising its two
optional dependencies' contracts ("Plugin file does not exist: --normalize-lidl": `installLidlContracts`,
`lib/common.nix` 255-275, given `modules_state` and `verified_proxy_module`). This drops the two
`optional_dependencies` from its metadata.json and has `rust-lib/src/glue.rs` call those two modules by
name through the SDK (`LogosModuleSDK::new().plugin(NAME).call_json_with_timeout(METHOD, ARGS, T)`,
what the generated clients do inside, logos-rust-sdk `lidl-gen/src/rustgen.rs` 414-512) instead of through
generated clients. Behaviour is unchanged: the verified-proxy gate and routing still ask the same methods
of the same modules. Fails loudly when the source is not the one expected.
"""
import json
import pathlib
import sys

src = pathlib.Path(sys.argv[1])
meta = src / "metadata.json"
m = json.loads(meta.read_text())
assert m["name"] == "eth_rpc_module" and m["version"] == "0.2.0", m.get("version")
m["optional_dependencies"] = []
meta.write_text(json.dumps(m, indent=2) + "\n")

glue = src / "rust-lib" / "src" / "glue.rs"
g = glue.read_text()
edits = [
    ("""        let listing = modules_state::ModulesStateClient::new()
            .list_modules_with_timeout(MODULES_STATE_BUDGET)
            .ok()
            .map(|l| l.to_json());""",
     """        let listing = LogosModuleSDK::new()
            .plugin("modules_state")
            .call_json_with_timeout("list_modules", &json!([]), MODULES_STATE_BUDGET)
            .ok();"""),
    ("""        verified_proxy_module::VerifiedProxyModuleClient::new()
            .status_with_timeout(PROBE_BUDGET)
            .map_err(|e| format!("{e:?}"))""",
     """        LogosModuleSDK::new()
            .plugin(PROXY_MODULE)
            .call_json_with_timeout("status", &json!([]), PROBE_BUDGET)
            .map_err(|e| format!("{e:?}"))"""),
    ("""        modules_state::ModulesStateClient::new()
            .module_record_with_timeout(PROXY_MODULE, MODULES_STATE_BUDGET)
            .ok()
            .flatten()
            .map(|r| r.to_json())""",
     """        LogosModuleSDK::new()
            .plugin("modules_state")
            .call_json_with_timeout("module_record", &json!([PROXY_MODULE]), MODULES_STATE_BUDGET)
            .ok()
            .filter(|r| !r.is_null())"""),
    ("""        let raw = verified_proxy_module::VerifiedProxyModuleClient::new()
            .rpc_with_timeout(method, params, left)""",
     """        let raw = LogosModuleSDK::new()
            .plugin(PROXY_MODULE)
            .call_json_with_timeout("rpc", &json!([method, params]), left)"""),
]
for old, new in edits:
    assert g.count(old) == 1, old[:60]
    g = g.replace(old, new)
assert "modules_state::" not in g and "verified_proxy_module::" not in g
glue.write_text(g)
print("eth_rpc_module: optional dependencies called by name (4 call sites)")
