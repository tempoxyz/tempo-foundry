use super::RawCallResult;

const SANCOV_BUFFER_CAPACITY: usize = 65536;

/// RAII guard that activates Tempo precompile coverage collection for the duration of an EVM call.
///
/// Allocates a thread-local scratch buffer for sancov hits and sets it as the active coverage map.
/// After execution, sancov hits are appended to the EVM edge coverage in `RawCallResult`.
pub(super) struct TempoCoverageGuard {
    collect_edges: bool,
}

thread_local! {
    static TEMPO_COV_BUFFER: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(vec![0u8; SANCOV_BUFFER_CAPACITY]);
}

impl TempoCoverageGuard {
    pub(super) fn new(collect_edges: bool, collect_trace_cmp: bool) -> Self {
        if collect_edges {
            TEMPO_COV_BUFFER.with(|buf| {
                let mut buf = buf.borrow_mut();
                buf.fill(0);
                let ptr = buf.as_mut_ptr();
                let len = buf.len();
                foundry_tempo_coverage::set_coverage_map(ptr, len);
            });
        }
        if collect_trace_cmp {
            foundry_tempo_coverage::clear_cmp_operands();
        }
        Self { collect_edges }
    }

    /// Populate the result's sancov coverage buffer with precompile edge hits.
    ///
    /// Sancov coverage is tracked independently from EVM edge coverage so that
    /// changes in the EVM edge map size cannot shift sancov IDs.
    pub(super) fn append_edges_into(result: &mut RawCallResult) {
        let sancov_used = foundry_tempo_coverage::sancov_edge_count();
        if sancov_used == 0 {
            return;
        }

        TEMPO_COV_BUFFER.with(|buf| {
            let buf = buf.borrow();
            let sancov_slice = &buf[..sancov_used.min(buf.len())];

            if !sancov_slice.iter().any(|&b| b > 0) {
                return;
            }

            result.sancov_coverage = Some(sancov_slice.to_vec());
        });
    }

    /// Drain captured comparison operands and attach them to the result for dictionary injection.
    pub(super) fn drain_cmp_into(result: &mut RawCallResult) {
        let cmp_values = foundry_tempo_coverage::drain_cmp_operands();
        if !cmp_values.is_empty() {
            result.tempo_cmp_values = Some(cmp_values);
        }
    }
}

impl Drop for TempoCoverageGuard {
    fn drop(&mut self) {
        if self.collect_edges {
            foundry_tempo_coverage::clear_coverage_map();
        }
    }
}
