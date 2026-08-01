//! Grep pipeline orchestration.

pub mod error;
pub mod input;

use crate::candidates::CandidateSource;
use crate::candidates::planner::CandidatePlanner;
use crate::candidates::scope::CandidateCoverage;
use crate::search::{
    EventEmission, Report, SearchInputs, SearchMode, SearchQuery, SearchSink, Searcher, StatsMode,
};
pub use crate::search::{InputConversion, Inputs};

pub use crate::corpus::Candidate;
pub use crate::corpus::candidate::PathDisplay;
pub use crate::corpus::filter::{
    CandidateFilter, CandidateFilterConfig, FilterAdmission, GlobConfig, HiddenMode, IgnoreConfig,
    IgnoreSources, TypeFilterRule, VisibilityConfig,
};
pub use crate::corpus::order::{CandidateOrder, CandidateOrderDirection, CandidateOrderKey};
pub use crate::corpus::walk::{AllFiles, FileWalk, WalkFile, WalkMetadata, WalkSelector};
pub use crate::index::Indexes;
pub use crate::search::CandidateTransform;
pub use error::Error;
pub use input::ByteInput;

pub struct Grep<'a> {
    source: CandidateSource<'a>,
}

pub struct GrepRequest<'a> {
    pub query: SearchQuery,
    pub streams: Inputs<'a>,
    pub conversion: InputConversion<'a>,
    pub mode: SearchMode,
    pub stats: StatsMode,
}

impl<'a> Grep<'a> {
    #[must_use]
    pub const fn new(source: CandidateSource<'a>) -> Self {
        Self { source }
    }

    /// Run a grep search and collect the requested report fields.
    ///
    /// # Errors
    ///
    /// Returns an error if query compilation, candidate resolution, or search fails.
    pub fn search(&self, request: GrepRequest<'a>) -> crate::Result<Report> {
        self.execute(request, EventEmission::Discard)
    }

    /// Run a grep search and emit semantic search events to a sink.
    ///
    /// # Errors
    ///
    /// Returns an error if query compilation, candidate resolution, search, or sink handling fails.
    pub fn stream(
        &self,
        request: GrepRequest<'a>,
        sink: &mut impl SearchSink,
    ) -> crate::Result<Report> {
        self.execute(request, EventEmission::Emit(sink))
    }

    /// Resolve corpus candidates for a search request without running search.
    ///
    /// Compiles the matcher first so Auto engine resolution affects narrowing.
    ///
    /// # Errors
    ///
    /// Returns an error if matcher construction or candidate resolution fails.
    pub fn resolve_candidates(
        &'a self,
        request: &GrepRequest<'_>,
    ) -> crate::Result<crate::Candidates<'a>> {
        let searcher = Searcher::new(request.query.clone())?;
        let coverage = CandidateCoverage::from_mode(request.mode);
        self.resolve_compiled(&searcher.query, coverage)
    }

    fn resolve_compiled(
        &'a self,
        query: &SearchQuery,
        coverage: CandidateCoverage,
    ) -> crate::Result<crate::Candidates<'a>> {
        let plan = CandidatePlanner::plan(&self.source, query, coverage);
        plan.resolve(&self.source)
    }

    fn execute(
        &self,
        request: GrepRequest<'a>,
        events: EventEmission<'_>,
    ) -> crate::Result<Report> {
        let GrepRequest {
            query,
            streams,
            conversion,
            mode,
            stats,
        } = request;
        let searcher = Searcher::new(query)?;
        let coverage = CandidateCoverage::from_mode(mode);
        let candidates = self.resolve_compiled(&searcher.query, coverage)?;
        let inputs = SearchInputs {
            candidates,
            streams,
            conversion,
        };
        searcher.execute(inputs, stats, mode, events)
    }
}

#[cfg(test)]
mod candidate_coverage_tests {
    use super::*;
    use crate::search::{SearchMode, ZeroCounts};

    #[test]
    fn count_lines_omit_uses_potential_matches() {
        assert_eq!(
            CandidateCoverage::from_mode(SearchMode::CountLines {
                zeros: ZeroCounts::Omit
            }),
            CandidateCoverage::PotentialMatches
        );
    }

    #[test]
    fn count_lines_include_uses_complete() {
        assert_eq!(
            CandidateCoverage::from_mode(SearchMode::CountLines {
                zeros: ZeroCounts::Include
            }),
            CandidateCoverage::Complete
        );
    }

    #[test]
    fn count_matches_omit_uses_potential_matches() {
        assert_eq!(
            CandidateCoverage::from_mode(SearchMode::CountMatches {
                zeros: ZeroCounts::Omit
            }),
            CandidateCoverage::PotentialMatches
        );
    }

    #[test]
    fn files_without_match_uses_complete() {
        assert_eq!(
            CandidateCoverage::from_mode(SearchMode::FilesWithoutMatch),
            CandidateCoverage::Complete
        );
    }
}
