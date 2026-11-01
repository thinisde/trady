//! Pure research calculations; deliberately disconnected from execution policy.

pub mod accounting;
pub mod attention;
pub mod coordination;
pub mod cross_asset;
pub mod event;
pub mod insider;
pub mod microstructure;
pub mod options;
pub mod short_interest;
pub mod tax;

use chrono::{DateTime, Utc};
use trady_domain::{
    Result,
    intelligence::{FeatureAssessment, FeatureInput},
};

pub fn assess(input: &FeatureInput, at: DateTime<Utc>) -> Result<FeatureAssessment> {
    Ok(match input {
        FeatureInput::Options(x) => FeatureAssessment::Options(options::assess(x, at)?),
        FeatureInput::Coordination(x) => {
            FeatureAssessment::Coordination(coordination::assess(x, at)?)
        }
        FeatureInput::Event(x) => FeatureAssessment::Event(event::assess(x, at)?),
        FeatureInput::ShortInterest(x) => {
            FeatureAssessment::ShortInterest(short_interest::assess(x, at)?)
        }
        FeatureInput::Insider(x) => FeatureAssessment::Insider(insider::assess(x, at)?),
        FeatureInput::CrossAsset(x) => FeatureAssessment::CrossAsset(cross_asset::assess(x, at)?),
        FeatureInput::Attention(x) => FeatureAssessment::Attention(attention::assess(x, at)?),
        FeatureInput::Accounting(x) => FeatureAssessment::Accounting(accounting::assess(x, at)?),
        FeatureInput::Microstructure(x) => {
            FeatureAssessment::Microstructure(microstructure::assess(x, at)?)
        }
        FeatureInput::Tax(x) => FeatureAssessment::Tax(Box::new(tax::assess(x, at)?)),
    })
}
