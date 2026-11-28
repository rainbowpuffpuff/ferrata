// Transaction outcomes on either LEZ wire: rc3's reverted-index list, or v0.4.0's receipt and charged fee.
import { el, badge } from "./ui.js";
import { fmtNative } from "./util.js";

export function receiptAt(block, index) {
  return block && Array.isArray(block.receipts) ? block.receipts[index] || null : null;
}

export function receiptFeeText(receipt, native) {
  const f = receipt.fee;
  return `fee ${fmtNative(f.fee_base + f.tip, native)} · refund ${fmtNative(f.refund, native)}`;
}

/** A badge (with the caller's rc3 wording), followed on v0.4.0 by the receipt's charged fee and refund. */
export function outcome(block, index, native, { applied = "applied", reverted = "reverted", system = "system" } = {}) {
  if (!block) return badge("—");
  if (!Array.isArray(block.receipts)) {
    return badge(block.reverted.includes(index) ? reverted : applied, block.reverted.includes(index) ? "bad" : "ok");
  }
  const r = receiptAt(block, index);
  if (!r) return badge(system, "info");
  return el("span", {}, badge(r.success ? applied : reverted, r.success ? "ok" : "bad"), " ",
    el("span", { class: "muted small" }, receiptFeeText(r, native)));
}
