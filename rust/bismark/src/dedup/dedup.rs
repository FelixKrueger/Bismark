//! Core dedup primitives.
//!
//! Pure logic, no I/O. The two types here are:
//!
//! - [`DedupKey`] — the value used to detect duplicates. Single key shape
//!   serves both single-end and paired-end (SE uses `end == start`), and is
//!   used for BOTH the seen-set and the positions-counter. That last point
//!   matters: a prior-art Rust port keyed the positions-counter on only
//!   `(strand, chr, start)` while keying the seen-set on the full
//!   `(strand, chr, start, end)`, producing a 97-position drift in the
//!   dedup report on a 10M PE WGBS dataset (~0.017%). Using the same key
//!   type for both eliminates that bug by construction.
//!
//! - [`DedupState`] — accumulates the seen-set, the duplicate-positions
//!   set, and the running counters. [`DedupState::observe`] returns
//!   `true` for unique records (caller should emit) and `false` for
//!   duplicates. Mirrors the Perl `deduplicate_bismark` `%unique_seqs` +
//!   `%positions` semantics from lines 500–512.
//!
//! See [`PLAN.md` §4.4 + §5](../../05242026_bismark-dedup-v1/PLAN.md) for
//! the byte-level mapping to Perl.

use crate::io::BismarkStrand;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::dedup::report::DedupReport;

/// Dedup key — `(strand, chr_id, start, end)`.
///
/// SE records use `end == start`; PE records use `end == reference_end`
/// on the appropriate mate (R2 for forward pairs, R1 for reverse).
///
/// `#[repr(C)]` together with the upstream `#[repr(u8)]` on
/// [`BismarkStrand`] pins the in-memory layout to a stable 16 bytes
/// (1 byte strand + 3 bytes padding + 3× `u32`). An anonymous
/// `const _: () = assert!(size_of::<DedupKey>() == 16);` below catches
/// any future drift at build time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct DedupKey {
    /// Bismark strand classification. For SE this is the per-record
    /// strand; for PE this is the pair-strand (R1-derived).
    pub strand: BismarkStrand,
    /// Chromosome interned index. Resolved from chr **name** (not noodles
    /// refID) so multi-file input via `--multiple` survives differing
    /// `@SQ` orderings across inputs.
    pub chr_id: u32,
    /// 1-based start position. For SE forward / PE forward-strand R1:
    /// the alignment start (SAM POS). For SE reverse / PE reverse-strand
    /// R2: the `alignment_start` of the mate that supplies the start.
    pub start: u32,
    /// 1-based inclusive end position. For SE forward: equals `start`.
    /// For SE reverse: `reference_end` of the record's CIGAR. For PE:
    /// `reference_end` of the mate that supplies the end.
    pub end: u32,
}

const _: () = {
    assert!(
        std::mem::size_of::<DedupKey>() == 16,
        "DedupKey must be exactly 16 bytes — see PLAN §5/§7 memory math",
    );
};

impl DedupKey {
    /// Construct an SE key. `end` is set to `key_pos` (SE composite is
    /// 3-tuple per Perl line 389).
    #[must_use]
    pub fn se(strand: BismarkStrand, chr_id: u32, key_pos: u32) -> Self {
        Self {
            strand,
            chr_id,
            start: key_pos,
            end: key_pos,
        }
    }

    /// Construct a PE key (4-tuple per Perl line 493).
    #[must_use]
    pub fn pe(strand: BismarkStrand, chr_id: u32, start: u32, end: u32) -> Self {
        Self {
            strand,
            chr_id,
            start,
            end,
        }
    }
}

/// Accumulating dedup state for a single input file or `--multiple` group.
///
/// Mirrors Perl `deduplicate_bismark`'s `%unique_seqs` (the seen-set) and
/// `%positions` (the duplicate-positions set) hashes. `count` and
/// `removed` are running totals; `n_positions()` returns the size of the
/// duplicate-positions set at the time of call.
#[derive(Debug, Default)]
pub struct DedupState {
    seen: FxHashSet<DedupKey>,
    duplicate_positions: FxHashSet<DedupKey>,
    count: u64,
    removed: u64,
}

impl DedupState {
    /// Construct an empty state. Use [`DedupState::with_capacity`] if you
    /// know the approximate record count up-front (saves rehashing).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-allocate the **seen-set** hash table to `capacity` records.
    /// Useful at the start of a large input — saves rehashing as the set
    /// grows. The duplicate-positions set is left at default capacity,
    /// since it's bounded by the number of distinct duplicate positions
    /// (typically much smaller than the seen-set on real data: e.g. on
    /// the 10M PE WGBS audit dataset, seen ≈ 8.0M but
    /// duplicate-positions ≈ 0.6M).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
            duplicate_positions: FxHashSet::default(),
            count: 0,
            removed: 0,
        }
    }

    /// Observe one record's dedup key. Returns `true` if the record is
    /// unique (caller should emit it) and `false` if it's a duplicate.
    ///
    /// Side effects:
    /// - Always increments `count`.
    /// - On duplicate: increments `removed` AND inserts into the
    ///   duplicate-positions set. The set's idempotent semantics mirror
    ///   Perl's `unless (exists $positions{$composite}) { ... }` guard
    ///   at lines 502–504.
    pub fn observe(&mut self, key: DedupKey) -> bool {
        self.count += 1;
        if self.seen.insert(key) {
            true
        } else {
            self.removed += 1;
            self.duplicate_positions.insert(key);
            false
        }
    }

    /// Total alignment records / pairs observed so far.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Number of records flagged as duplicates so far.
    #[must_use]
    pub fn removed(&self) -> u64 {
        self.removed
    }

    /// Number of **distinct positions** at which at least one duplicate
    /// was seen. Used directly as the "different position(s)" number in
    /// the dedup report.
    #[must_use]
    pub fn n_positions(&self) -> usize {
        self.duplicate_positions.len()
    }

    /// Consume the state into a [`DedupReport`] bound to the given file
    /// label (typically the input path as supplied on the CLI — Perl
    /// echoes `$ARGV[i]` verbatim in the report).
    #[must_use]
    pub fn into_report(self, file_label: String) -> DedupReport {
        DedupReport::new(
            file_label,
            self.count,
            self.removed,
            self.duplicate_positions.len(),
            false, // non-UMI mode
        )
    }
}

// ────────────────────────────────────────────────────────────────────────
// Phase B (v1.2 UMI epic): UMI-aware dedup key + state.
//
// UmiDedupKey is the UMI-mode sibling of DedupKey. The position-only
// dedup path (DedupKey + DedupState above) is unchanged — non-UMI
// workflows continue to use 16-byte keys with the existing
// compile-time-asserted layout. UMI workflows use the wider UmiDedupKey
// below. The pipeline picks the appropriate state container based on
// CLI `--barcode` / `--bclconvert` flags.
//
// See PLAN.md Phase B §3.2-3.3 for the two-HashSets rationale.
// ────────────────────────────────────────────────────────────────────────

/// UMI-aware dedup key — `(strand, chr_id, start, end, umi)`.
///
/// Mirrors Perl `deduplicate_bismark`'s `deduplicate_barcoded_rrbs` key
/// formula: position + UMI bytes. Two records at the same position with
/// different UMIs are distinct (correct UMI extraction prevents
/// over-dedup of independent template molecules that happened to land
/// at the same chromosome coordinates).
///
/// UMI storage uses [`SmallVec<[u8; 16]>`] — stack-allocated for ≤16-byte
/// UMIs (all known Bismark workflows; covers single 8-mer ACGT, 6-mer,
/// 10-mer, and 8+8-mer dual-UMI without the `+`) with transparent heap
/// fallback for longer UMIs (e.g. dual-UMI `XXXXXXXX+YYYYYYYY` at 17
/// bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UmiDedupKey {
    /// Bismark strand classification. Same semantics as [`DedupKey::strand`].
    pub strand: BismarkStrand,
    /// Chromosome interned index. Same semantics as [`DedupKey::chr_id`].
    pub chr_id: u32,
    /// 1-based start position. Same semantics as [`DedupKey::start`].
    pub start: u32,
    /// 1-based inclusive end position. Same semantics as [`DedupKey::end`].
    pub end: u32,
    /// Extracted UMI bytes (from qname via the user-selected extractor).
    /// Never empty for records that survive the dedup pipeline's UMI
    /// extraction step — empty UMIs are caught upstream as
    /// `UmiExtractionFailed`.
    pub umi: SmallVec<[u8; 16]>,
}

// Cap UmiDedupKey at 64 bytes inline. Reviewer B flagged a 24-vs-32 disagreement
// on the SmallVec size; this assertion lets the compiler enforce the
// upper bound. If it ever fires, the size_of has crept past expectations
// and the storage strategy needs revisiting.
const _: () = {
    assert!(
        std::mem::size_of::<UmiDedupKey>() <= 64,
        "UmiDedupKey grew past 64 bytes — review the SmallVec inline capacity \
         and per-key memory budget in PLAN §8 before relaxing this bound",
    );
};

impl UmiDedupKey {
    /// Construct an SE UMI key (mirrors [`DedupKey::se`]).
    #[must_use]
    pub fn se(strand: BismarkStrand, chr_id: u32, key_pos: u32, umi: SmallVec<[u8; 16]>) -> Self {
        Self {
            strand,
            chr_id,
            start: key_pos,
            end: key_pos,
            umi,
        }
    }

    /// Construct a PE UMI key (mirrors [`DedupKey::pe`]).
    #[must_use]
    pub fn pe(
        strand: BismarkStrand,
        chr_id: u32,
        start: u32,
        end: u32,
        umi: SmallVec<[u8; 16]>,
    ) -> Self {
        Self {
            strand,
            chr_id,
            start,
            end,
            umi,
        }
    }
}

/// UMI-aware dedup state — sibling of [`DedupState`] for UMI mode.
///
/// Same `observe()` semantics: returns `true` for unique records (caller
/// should emit), `false` for duplicates. Same report shape via
/// [`UmiDedupState::into_report`] — the only difference vs the
/// position-only path is the `(UMI mode)` banner suffix in the report.
#[derive(Debug, Default)]
pub struct UmiDedupState {
    seen: FxHashSet<UmiDedupKey>,
    duplicate_positions: FxHashSet<UmiDedupKey>,
    count: u64,
    removed: u64,
}

impl UmiDedupState {
    /// Construct an empty UMI dedup state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-allocate the seen-set to `capacity` records.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
            duplicate_positions: FxHashSet::default(),
            count: 0,
            removed: 0,
        }
    }

    /// Observe one record's UMI dedup key. Returns `true` if unique (caller
    /// should emit), `false` if duplicate. Same Perl-`%unique_seqs` +
    /// `%positions` semantics as [`DedupState::observe`].
    pub fn observe(&mut self, key: UmiDedupKey) -> bool {
        self.count += 1;
        if self.seen.insert(key.clone()) {
            true
        } else {
            self.removed += 1;
            self.duplicate_positions.insert(key);
            false
        }
    }

    /// Total alignment records / pairs observed so far.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Number of records flagged as duplicates so far.
    #[must_use]
    pub fn removed(&self) -> u64 {
        self.removed
    }

    /// Number of distinct positions (UMI-aware) at which at least one
    /// duplicate was seen.
    #[must_use]
    pub fn n_positions(&self) -> usize {
        self.duplicate_positions.len()
    }

    /// Consume into a [`DedupReport`] with `umi_mode = true` (emits the
    /// `(UMI mode)` banner suffix per `deduplicate_bismark:908`).
    #[must_use]
    pub fn into_report(self, file_label: String) -> DedupReport {
        DedupReport::new(
            file_label,
            self.count,
            self.removed,
            self.duplicate_positions.len(),
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ot_key(start: u32, end: u32) -> DedupKey {
        DedupKey::pe(BismarkStrand::OT, 0, start, end)
    }

    #[test]
    fn se_first_occurrence_unique_second_duplicate() {
        let mut state = DedupState::new();
        let key = DedupKey::se(BismarkStrand::OT, 0, 100);
        assert!(state.observe(key), "first occurrence must be unique");
        assert!(!state.observe(key), "second occurrence must be a duplicate");
        assert_eq!(state.count(), 2);
        assert_eq!(state.removed(), 1);
        assert_eq!(
            state.n_positions(),
            1,
            "exactly one position seen as duplicate"
        );
    }

    /// Alan's 97-position drift regression test.
    ///
    /// Two PE keys with the same `(strand, chr, start)` but differing
    /// `end` MUST be treated as distinct positions — both in the
    /// seen-set AND in the duplicate-positions counter.
    ///
    /// The prior-art Rust port keyed the duplicate-positions counter on
    /// `(strand, chr, start)` only via `pack_pos_pe`, causing two
    /// genuinely distinct PE pairs to collapse to one position.
    #[test]
    fn alan_drift_regression_distinct_ends_distinct_positions() {
        let mut state = DedupState::new();
        let key_a = ot_key(100, 200);
        let key_b = ot_key(100, 250);
        // First occurrences — both unique.
        assert!(state.observe(key_a));
        assert!(state.observe(key_b));
        // Second occurrences — both duplicates, must increment n_positions to 2.
        assert!(!state.observe(key_a));
        assert!(!state.observe(key_b));
        assert_eq!(state.count(), 4);
        assert_eq!(state.removed(), 2);
        assert_eq!(
            state.n_positions(),
            2,
            "two distinct (strand,chr,start,end) tuples produced two distinct positions"
        );
    }

    #[test]
    fn duplicate_positions_set_is_idempotent_on_repeat_duplicates() {
        // Mirrors Perl's `unless (exists $positions{$composite})` guard:
        // seeing the same composite three+ times still only counts as one
        // distinct duplicate-position.
        let mut state = DedupState::new();
        let key = DedupKey::se(BismarkStrand::OT, 0, 100);
        state.observe(key); // unique
        state.observe(key); // dup 1
        state.observe(key); // dup 2
        state.observe(key); // dup 3
        assert_eq!(state.count(), 4);
        assert_eq!(state.removed(), 3);
        assert_eq!(
            state.n_positions(),
            1,
            "all dups at same key → one position"
        );
    }

    #[test]
    fn empty_state_zero_counters() {
        let state = DedupState::new();
        assert_eq!(state.count(), 0);
        assert_eq!(state.removed(), 0);
        assert_eq!(state.n_positions(), 0);
    }

    #[test]
    fn keys_with_different_strands_are_distinct() {
        let mut state = DedupState::new();
        let ot = DedupKey::se(BismarkStrand::OT, 0, 100);
        let ob = DedupKey::se(BismarkStrand::OB, 0, 100);
        assert!(state.observe(ot));
        assert!(state.observe(ob), "different strand → not a duplicate");
        assert_eq!(state.count(), 2);
        assert_eq!(state.removed(), 0);
    }

    #[test]
    fn keys_with_different_chr_ids_are_distinct() {
        let mut state = DedupState::new();
        let chr0 = DedupKey::se(BismarkStrand::OT, 0, 100);
        let chr1 = DedupKey::se(BismarkStrand::OT, 1, 100);
        assert!(state.observe(chr0));
        assert!(state.observe(chr1));
        assert_eq!(state.removed(), 0);
    }

    #[test]
    fn se_constructor_sets_end_equal_to_start() {
        let key = DedupKey::se(BismarkStrand::OT, 5, 42);
        assert_eq!(key.start, 42);
        assert_eq!(key.end, 42);
    }

    #[test]
    fn pe_constructor_preserves_distinct_start_and_end() {
        let key = DedupKey::pe(BismarkStrand::CTOT, 3, 100, 250);
        assert_eq!(key.start, 100);
        assert_eq!(key.end, 250);
        assert_eq!(key.strand, BismarkStrand::CTOT);
    }

    // ─── Phase B (v1.2 UMI epic): UmiDedupKey / UmiDedupState tests ────

    fn umi(bytes: &[u8]) -> SmallVec<[u8; 16]> {
        SmallVec::from_slice(bytes)
    }

    #[test]
    fn umi_dedup_key_same_position_different_umi_are_distinct() {
        // Two records at the same (strand, chr, start, end) but with
        // different UMIs MUST be treated as unique by the seen-set. This
        // is the entire point of UMI mode — independent template
        // molecules that happen to land at the same coordinates should
        // not collapse to one.
        let mut state = UmiDedupState::new();
        let k1 = UmiDedupKey::pe(BismarkStrand::OT, 0, 100, 200, umi(b"AAAAAAAA"));
        let k2 = UmiDedupKey::pe(BismarkStrand::OT, 0, 100, 200, umi(b"TTTTTTTT"));
        assert!(state.observe(k1));
        assert!(state.observe(k2));
        assert_eq!(state.count(), 2);
        assert_eq!(state.removed(), 0, "different UMIs → not duplicates");
    }

    #[test]
    fn umi_dedup_key_same_position_same_umi_is_duplicate() {
        let mut state = UmiDedupState::new();
        let k1 = UmiDedupKey::pe(BismarkStrand::OT, 0, 100, 200, umi(b"AAAAAAAA"));
        let k2 = UmiDedupKey::pe(BismarkStrand::OT, 0, 100, 200, umi(b"AAAAAAAA"));
        assert!(state.observe(k1));
        assert!(
            !state.observe(k2),
            "identical UMIs at same position → duplicate"
        );
        assert_eq!(state.count(), 2);
        assert_eq!(state.removed(), 1);
        assert_eq!(state.n_positions(), 1);
    }

    #[test]
    fn umi_dedup_state_reports_with_umi_mode_banner() {
        // UmiDedupState::into_report() must set the umi_mode flag so the
        // report's banner gets the `(UMI mode)` suffix (matches Perl
        // line 908).
        let mut state = UmiDedupState::new();
        state.observe(UmiDedupKey::pe(
            BismarkStrand::OT,
            0,
            100,
            200,
            umi(b"AAAAAAAA"),
        ));
        let report = state.into_report("/path/sample.bam".to_string());
        let formatted = report.format();
        assert!(
            formatted.contains("(UMI mode):"),
            "report must contain `(UMI mode)` banner, got: {formatted:?}"
        );
    }

    #[test]
    fn umi_dedup_key_dual_umi_with_plus_uses_heap_path_correctly() {
        // Dual-UMI `XXXXXXXX+YYYYYYYY` is 17 bytes — exceeds the
        // SmallVec inline capacity. The heap fallback must still hash +
        // compare correctly.
        let dual_umi: SmallVec<[u8; 16]> = SmallVec::from_slice(b"AAAAAAAA+TTTTTTTT");
        assert_eq!(dual_umi.len(), 17);
        assert!(dual_umi.spilled(), "17-byte UMI must spill to heap");
        let mut state = UmiDedupState::new();
        let k1 = UmiDedupKey::pe(BismarkStrand::OT, 0, 100, 200, dual_umi.clone());
        let k2 = UmiDedupKey::pe(BismarkStrand::OT, 0, 100, 200, dual_umi);
        assert!(state.observe(k1));
        assert!(!state.observe(k2), "same dual-UMI must be a duplicate");
    }

    #[test]
    fn umi_dedup_key_size_under_64_bytes() {
        // The compile-time `const _` assertion guarantees this at build
        // time. The runtime check below mostly documents the actual size
        // for the CHANGELOG note.
        let actual = std::mem::size_of::<UmiDedupKey>();
        assert!(
            actual <= 64,
            "UmiDedupKey size {actual} > 64; the const-assert should have caught this"
        );
    }

    #[test]
    fn umi_dedup_state_empty_zero_counters() {
        let state = UmiDedupState::new();
        assert_eq!(state.count(), 0);
        assert_eq!(state.removed(), 0);
        assert_eq!(state.n_positions(), 0);
    }

    #[test]
    fn umi_se_constructor_sets_end_equal_to_start() {
        let key = UmiDedupKey::se(BismarkStrand::OT, 5, 42, umi(b"BARCODE"));
        assert_eq!(key.start, 42);
        assert_eq!(key.end, 42);
        assert_eq!(key.umi.as_slice(), b"BARCODE");
    }
}
