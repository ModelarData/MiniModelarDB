/* Copyright 2022 The MiniModelarDB Contributors
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

//! Support for different kinds of stored segments.
//!
//! The main SegmentBuilder struct provides support for inserting and storing data in a in-memory
//! segment. Furthermore, the data can be retrieved as a structured record batch.

use crate::storage::data_point::DataPoint;
use crate::storage::{MetaData, Timestamp, INITIAL_BUILDER_CAPACITY};
use datafusion::arrow::array::{
    ArrayBuilder, Float32Array, PrimitiveBuilder, TimestampMicrosecondArray,
};
use datafusion::arrow::datatypes::TimeUnit::Microsecond;
use datafusion::arrow::datatypes::{
    DataType, Field, Float32Type, Schema, TimestampMicrosecondType,
};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::basic::Encoding;
use datafusion::parquet::file::properties::WriterProperties;
use std::fmt::Formatter;
use std::fs::File;
use std::sync::Arc;
use std::{fmt, fs};

/// A single segment being built, consisting of a series of timestamps and values. Note that
/// since array builders are used, the data can only be read once the builders are finished and
/// can not be further appended to after.
pub struct SegmentBuilder {
    /// Builder consisting of timestamps with microsecond precision.
    timestamps: PrimitiveBuilder<TimestampMicrosecondType>,
    /// Builder consisting of float values.
    values: PrimitiveBuilder<Float32Type>,
    /// Metadata used to uniquely identify the segment (and related sensor).
    pub metadata: MetaData,
    /// First timestamp used to uniquely identify the segment from other from the same sensor.
    first_timestamp: Timestamp,
}

impl fmt::Display for SegmentBuilder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&*format!("Segment with {} data point(s) (", self.timestamps.len()));
        f.write_str(&*format!("timestamp capacity: {}, ", self.timestamps.capacity()));
        f.write_str(&*format!("values capacity: {})", self.values.capacity()));

        Ok(())
    }
}

impl SegmentBuilder {
    pub fn new(data_point: &DataPoint) -> Self {
        Self {
            // Note that the actual internal capacity might be slightly larger than these values. Apache
            // arrow defines the argument as being the lower bound for how many items the builder can hold.
            timestamps: TimestampMicrosecondArray::builder(INITIAL_BUILDER_CAPACITY),
            values: Float32Array::builder(INITIAL_BUILDER_CAPACITY),
            metadata: data_point.metadata.to_vec(),
            first_timestamp: data_point.timestamp,
        }
    }

    /// If at least one of the builders are at capacity, return true.
    pub fn is_full(&self) -> bool {
        // The length is always the same for both builders.
        let length = self.timestamps.len();

        length == self.timestamps.capacity() || length == self.values.capacity()
    }

    /// Add the timestamp and value from the data point to the segment array builders.
    pub fn insert_data(&mut self, data_point: &DataPoint) {
        self.timestamps.append_value(data_point.timestamp).unwrap();
        self.values.append_value(data_point.value).unwrap();

        println!("Inserted data point into {}.", self)
    }

    /// Finish the array builders and return the data in a structured record batch.
    pub fn get_data(&mut self) -> RecordBatch {
        let timestamps = self.timestamps.finish();
        let values = self.values.finish();

        let schema = Schema::new(vec![
            Field::new("timestamps", DataType::Timestamp(Microsecond, None), false),
            Field::new("values", DataType::Float32, false),
        ]);

        RecordBatch::try_new(
            Arc::new(schema),
            vec![Arc::new(timestamps), Arc::new(values)]
        ).unwrap()
    }

    /// Write `data` to persistent parquet file storage.
    pub fn save_compressed_data(&self, data: RecordBatch) {
        let folder_name = self.metadata.join("-");
        fs::create_dir_all(&folder_name);

        let path = format!("{}/{}.parquet", folder_name, self.first_timestamp);
        write_batch_to_parquet(data, path);
    }
}

/// Write `batch` to a parquet file at the location given by `path`.
fn write_batch_to_parquet(batch: RecordBatch, path: String) {
    // Write the record batch to the parquet file buffer.
    let file = File::create(path).unwrap();
    let props = WriterProperties::builder()
        .set_dictionary_enabled(false)
        // TODO: Test using more efficient encoding. Plain encoding makes it easier to read the files externally.
        .set_encoding(Encoding::PLAIN)
        .build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props)).unwrap();

    writer.write(&batch).expect("Writing batch.");
    writer.close().unwrap();
}

// TODO: Test checking if an empty segment is full.
// TODO: Test checking if a full segment is full.
// TODO: Test inserting a data point into the segment.
// TODO: Test Getting the data from a segment.
// TODO: Maybe test saving the compressed data (how to test saving files?)