/* Copyright 2021 The MiniModelarDB Contributors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Implementation of the model types used for compressing time series segments
//! as models and functions for efficient computation of aggregates for models
//! of type PMC-Mean and Swing. The module itself contains functionality used by
//! multiple of the model types.

mod gorilla;
mod pmcmean;
mod swing;

use std::cmp::{Ordering, PartialOrd};

use datafusion::arrow::array::{
    Float32Builder, Int32Array, Int32Builder, Int64Array, TimestampMillisecondBuilder,
};
use crate::types::{
    TimeSeriesId, TimeSeriesIdBuilder, TimeSeriesIdArray, Timestamp, TimestampBuilder, TimestampArray, Value, ValueBuilder, ValueArray,
};

use crate::errors::MiniModelarDBError;

// TODO: replace Int64Array with TimestampArray when refactoring query engine.

/// Unique ids for each model type. Constant values are used instead of an enum
/// so the stored model type ids can be used in match expressions without being
/// converted to an enum first.
const PMC_MEAN_ID: u8 = 2;
const SWING_ID: u8 = 3;
const GORILLA_ID: u8 = 4;

/// General error bound that is guaranteed to not be negative, infinite, or NaN.
/// For `PMCMean` and `Swing` the error bound is interpreted as a relative per
/// value error bound in percentage. `Gorilla` is lossless.
struct ErrorBound(f32);

impl ErrorBound {
    /// Return `Self` if `error_bound` is a positive finite value, otherwise
    /// `CompressionError`.
    fn try_new(error_bound: f32) -> Result<Self, MiniModelarDBError> {
        if error_bound < 0.0 || error_bound.is_infinite() || error_bound.is_nan() {
            Err(MiniModelarDBError::CompressionError(
                "Error bound cannot be negative, infinite, or NaN".to_owned(),
            ))
        } else {
            Ok(Self(error_bound))
        }
    }
}

impl PartialEq<ErrorBound> for f32 {
    fn eq(&self, other: &ErrorBound) -> bool {
        self.eq(&other.0)
    }
}

impl PartialOrd<ErrorBound> for f32 {
    fn partial_cmp(&self, other: &ErrorBound) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

pub fn grid(
    tid: TimeSeriesId,
    start_time: Timestamp,
    end_time: Timestamp,
    mtid: i32,
    sampling_interval: i32,
    model: &[u8],
    gaps: &[u8],
    tids: &mut TimeSeriesIdBuilder,
    timestamps: &mut TimestampBuilder,
    values: &mut ValueBuilder,
) {
    match mtid as u8 {
        PMC_MEAN_ID => pmcmean::grid(
            tid,
            start_time,
            end_time,
            sampling_interval,
            model,
            gaps,
            tids,
            timestamps,
            values,
        ),
        SWING_ID => swing::grid(
            tid,
            start_time,
            end_time,
            sampling_interval,
            model,
            gaps,
            tids,
            timestamps,
            values,
        ),
        GORILLA_ID => gorilla::grid(
            tid,
            start_time,
            end_time,
            sampling_interval,
            model,
            gaps,
            tids,
            timestamps,
            values,
        ),
        _ => panic!("Internal error due to unknown model type"),
    }
}

/// Assumes all arrays are the same length and contain less or equal to num_rows elements.
pub fn count(
    num_rows: usize,
    tids: &TimeSeriesIdArray,
    start_times: &Int64Array ,
    end_times: &Int64Array ,
    sampling_intervals: &Int32Array,
) -> usize {
    let mut data_points = 0;
    for row_index in 0..num_rows {
        let tid = tids.value(row_index) as usize;
        let sampling_interval = sampling_intervals.value(tid) as i64;
        data_points +=
            ((end_times.value(row_index) - start_times.value(row_index)) / sampling_interval) + 1;
    }
    data_points as usize
}

pub fn min(
    tid: TimeSeriesId,
    start_time: Timestamp,
    end_time: Timestamp,
    mtid: i32,
    sampling_interval: i32,
    model: &[u8],
    gaps: &[u8],
) -> Value {
    match mtid as u8 {
        PMC_MEAN_ID => pmcmean::min_max(tid, start_time, end_time, sampling_interval, model, gaps),
        SWING_ID => swing::min(tid, start_time, end_time, sampling_interval, model, gaps),
        GORILLA_ID => gorilla::min(tid, start_time, end_time, sampling_interval, model, gaps),
        _ => panic!("Internal error due to unknown model type"),
    }
}

pub fn max(
    tid: TimeSeriesId,
    start_time: Timestamp,
    end_time: Timestamp,
    mtid: i32,
    sampling_interval: i32,
    model: &[u8],
    gaps: &[u8],
) -> Value {
    match mtid as u8 {
        PMC_MEAN_ID => pmcmean::min_max(tid, start_time, end_time, sampling_interval, model, gaps),
        SWING_ID => swing::max(tid, start_time, end_time, sampling_interval, model, gaps),
        GORILLA_ID => gorilla::max(tid, start_time, end_time, sampling_interval, model, gaps),
        _ => panic!("Internal error due to unknown model type"),
    }
}

pub fn sum(
    tid: TimeSeriesId,
    start_time: Timestamp,
    end_time: Timestamp,
    mtid: i32,
    sampling_interval: i32,
    model: &[u8],
    gaps: &[u8],
) -> Value {
    match mtid as u8 {
        PMC_MEAN_ID => pmcmean::sum(tid, start_time, end_time, sampling_interval, model, gaps),
        SWING_ID => swing::sum(tid, start_time, end_time, sampling_interval, model, gaps),
        GORILLA_ID => gorilla::sum(tid, start_time, end_time, sampling_interval, model, gaps),
        _ => panic!("Internal error due to unknown model type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::num;
    use proptest::proptest;

    proptest! {
    #[test]
    fn test_error_bound_can_be_positive(error_bound in num::f32::POSITIVE) {
        assert!(ErrorBound::try_new(error_bound).is_ok())
    }

    #[test]
    fn test_error_bound_cannot_be_negative(error_bound in num::f32::NEGATIVE) {
        assert!(ErrorBound::try_new(error_bound).is_err())
    }
    }

    #[test]
    fn test_error_bound_cannot_be_positive_infinity() {
        assert!(ErrorBound::try_new(f32::INFINITY).is_err())
    }

    #[test]
    fn test_error_bound_cannot_be_negative_infinity() {
        assert!(ErrorBound::try_new(f32::NEG_INFINITY).is_err())
    }

    #[test]
    fn test_error_bound_cannot_be_nan() {
        assert!(ErrorBound::try_new(f32::NAN).is_err())
    }
}
