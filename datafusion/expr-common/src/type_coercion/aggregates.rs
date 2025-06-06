// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use crate::signature::TypeSignature;
use arrow::datatypes::{
    DataType, Field, TimeUnit, DECIMAL128_MAX_PRECISION, DECIMAL128_MAX_SCALE,
    DECIMAL256_MAX_PRECISION, DECIMAL256_MAX_SCALE,
};

use datafusion_common::{internal_err, plan_err, Result};

pub static STRINGS: &[DataType] =
    &[DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View];

pub static SIGNED_INTEGERS: &[DataType] = &[
    DataType::Int8,
    DataType::Int16,
    DataType::Int32,
    DataType::Int64,
];

pub static UNSIGNED_INTEGERS: &[DataType] = &[
    DataType::UInt8,
    DataType::UInt16,
    DataType::UInt32,
    DataType::UInt64,
];

pub static INTEGERS: &[DataType] = &[
    DataType::Int8,
    DataType::Int16,
    DataType::Int32,
    DataType::Int64,
    DataType::UInt8,
    DataType::UInt16,
    DataType::UInt32,
    DataType::UInt64,
];

pub static NUMERICS: &[DataType] = &[
    DataType::Int8,
    DataType::Int16,
    DataType::Int32,
    DataType::Int64,
    DataType::UInt8,
    DataType::UInt16,
    DataType::UInt32,
    DataType::UInt64,
    DataType::Float32,
    DataType::Float64,
];

pub static TIMESTAMPS: &[DataType] = &[
    DataType::Timestamp(TimeUnit::Second, None),
    DataType::Timestamp(TimeUnit::Millisecond, None),
    DataType::Timestamp(TimeUnit::Microsecond, None),
    DataType::Timestamp(TimeUnit::Nanosecond, None),
];

pub static DATES: &[DataType] = &[DataType::Date32, DataType::Date64];

pub static BINARYS: &[DataType] = &[DataType::Binary, DataType::LargeBinary];

pub static TIMES: &[DataType] = &[
    DataType::Time32(TimeUnit::Second),
    DataType::Time32(TimeUnit::Millisecond),
    DataType::Time64(TimeUnit::Microsecond),
    DataType::Time64(TimeUnit::Nanosecond),
];

/// Validate the length of `input_fields` matches the `signature` for `agg_fun`.
///
/// This method DOES NOT validate the argument fields - only that (at least one,
/// in the case of [`TypeSignature::OneOf`]) signature matches the desired
/// number of input types.
pub fn check_arg_count(
    func_name: &str,
    input_fields: &[Field],
    signature: &TypeSignature,
) -> Result<()> {
    match signature {
        TypeSignature::Uniform(agg_count, _) | TypeSignature::Any(agg_count) => {
            if input_fields.len() != *agg_count {
                return plan_err!(
                    "The function {func_name} expects {:?} arguments, but {:?} were provided",
                    agg_count,
                    input_fields.len()
                );
            }
        }
        TypeSignature::Exact(types) => {
            if types.len() != input_fields.len() {
                return plan_err!(
                    "The function {func_name} expects {:?} arguments, but {:?} were provided",
                    types.len(),
                    input_fields.len()
                );
            }
        }
        TypeSignature::OneOf(variants) => {
            let ok = variants
                .iter()
                .any(|v| check_arg_count(func_name, input_fields, v).is_ok());
            if !ok {
                return plan_err!(
                    "The function {func_name} does not accept {:?} function arguments.",
                    input_fields.len()
                );
            }
        }
        TypeSignature::VariadicAny => {
            if input_fields.is_empty() {
                return plan_err!(
                    "The function {func_name} expects at least one argument"
                );
            }
        }
        TypeSignature::UserDefined
        | TypeSignature::Numeric(_)
        | TypeSignature::Coercible(_) => {
            // User-defined signature is validated in `coerce_types`
            // Numeric and Coercible signature is validated in `get_valid_types`
        }
        _ => {
            return internal_err!(
                "Aggregate functions do not support this {signature:?}"
            );
        }
    }
    Ok(())
}

/// Function return type of a sum
pub fn sum_return_type(arg_type: &DataType) -> Result<DataType> {
    match arg_type {
        DataType::Int64 => Ok(DataType::Int64),
        DataType::UInt64 => Ok(DataType::UInt64),
        DataType::Float64 => Ok(DataType::Float64),
        DataType::Decimal128(precision, scale) => {
            // In the spark, the result type is DECIMAL(min(38,precision+10), s)
            // Ref: https://github.com/apache/spark/blob/fcf636d9eb8d645c24be3db2d599aba2d7e2955a/sql/catalyst/src/main/scala/org/apache/spark/sql/catalyst/expressions/aggregate/Sum.scala#L66
            let new_precision = DECIMAL128_MAX_PRECISION.min(*precision + 10);
            Ok(DataType::Decimal128(new_precision, *scale))
        }
        DataType::Decimal256(precision, scale) => {
            // In the spark, the result type is DECIMAL(min(38,precision+10), s)
            // Ref: https://github.com/apache/spark/blob/fcf636d9eb8d645c24be3db2d599aba2d7e2955a/sql/catalyst/src/main/scala/org/apache/spark/sql/catalyst/expressions/aggregate/Sum.scala#L66
            let new_precision = DECIMAL256_MAX_PRECISION.min(*precision + 10);
            Ok(DataType::Decimal256(new_precision, *scale))
        }
        other => plan_err!("SUM does not support type \"{other:?}\""),
    }
}

/// Function return type of variance
pub fn variance_return_type(arg_type: &DataType) -> Result<DataType> {
    if NUMERICS.contains(arg_type) {
        Ok(DataType::Float64)
    } else {
        plan_err!("VAR does not support {arg_type:?}")
    }
}

/// Function return type of covariance
pub fn covariance_return_type(arg_type: &DataType) -> Result<DataType> {
    if NUMERICS.contains(arg_type) {
        Ok(DataType::Float64)
    } else {
        plan_err!("COVAR does not support {arg_type:?}")
    }
}

/// Function return type of correlation
pub fn correlation_return_type(arg_type: &DataType) -> Result<DataType> {
    if NUMERICS.contains(arg_type) {
        Ok(DataType::Float64)
    } else {
        plan_err!("CORR does not support {arg_type:?}")
    }
}

/// Function return type of an average
pub fn avg_return_type(func_name: &str, arg_type: &DataType) -> Result<DataType> {
    match arg_type {
        DataType::Decimal128(precision, scale) => {
            // In the spark, the result type is DECIMAL(min(38,precision+4), min(38,scale+4)).
            // Ref: https://github.com/apache/spark/blob/fcf636d9eb8d645c24be3db2d599aba2d7e2955a/sql/catalyst/src/main/scala/org/apache/spark/sql/catalyst/expressions/aggregate/Average.scala#L66
            let new_precision = DECIMAL128_MAX_PRECISION.min(*precision + 4);
            let new_scale = DECIMAL128_MAX_SCALE.min(*scale + 4);
            Ok(DataType::Decimal128(new_precision, new_scale))
        }
        DataType::Decimal256(precision, scale) => {
            // In the spark, the result type is DECIMAL(min(38,precision+4), min(38,scale+4)).
            // Ref: https://github.com/apache/spark/blob/fcf636d9eb8d645c24be3db2d599aba2d7e2955a/sql/catalyst/src/main/scala/org/apache/spark/sql/catalyst/expressions/aggregate/Average.scala#L66
            let new_precision = DECIMAL256_MAX_PRECISION.min(*precision + 4);
            let new_scale = DECIMAL256_MAX_SCALE.min(*scale + 4);
            Ok(DataType::Decimal256(new_precision, new_scale))
        }
        DataType::Duration(time_unit) => Ok(DataType::Duration(*time_unit)),
        arg_type if NUMERICS.contains(arg_type) => Ok(DataType::Float64),
        DataType::Dictionary(_, dict_value_type) => {
            avg_return_type(func_name, dict_value_type.as_ref())
        }
        other => plan_err!("{func_name} does not support {other:?}"),
    }
}

/// Internal sum type of an average
pub fn avg_sum_type(arg_type: &DataType) -> Result<DataType> {
    match arg_type {
        DataType::Decimal128(precision, scale) => {
            // In the spark, the sum type of avg is DECIMAL(min(38,precision+10), s)
            let new_precision = DECIMAL128_MAX_PRECISION.min(*precision + 10);
            Ok(DataType::Decimal128(new_precision, *scale))
        }
        DataType::Decimal256(precision, scale) => {
            // In Spark the sum type of avg is DECIMAL(min(38,precision+10), s)
            let new_precision = DECIMAL256_MAX_PRECISION.min(*precision + 10);
            Ok(DataType::Decimal256(new_precision, *scale))
        }
        DataType::Duration(time_unit) => Ok(DataType::Duration(*time_unit)),
        arg_type if NUMERICS.contains(arg_type) => Ok(DataType::Float64),
        DataType::Dictionary(_, dict_value_type) => {
            avg_sum_type(dict_value_type.as_ref())
        }
        other => plan_err!("AVG does not support {other:?}"),
    }
}

pub fn is_sum_support_arg_type(arg_type: &DataType) -> bool {
    match arg_type {
        DataType::Dictionary(_, dict_value_type) => {
            is_sum_support_arg_type(dict_value_type.as_ref())
        }
        _ => matches!(
            arg_type,
            arg_type if NUMERICS.contains(arg_type)
            || matches!(arg_type, DataType::Decimal128(_, _) | DataType::Decimal256(_, _))
        ),
    }
}

pub fn is_avg_support_arg_type(arg_type: &DataType) -> bool {
    match arg_type {
        DataType::Dictionary(_, dict_value_type) => {
            is_avg_support_arg_type(dict_value_type.as_ref())
        }
        _ => matches!(
            arg_type,
            arg_type if NUMERICS.contains(arg_type)
                || matches!(arg_type, DataType::Decimal128(_, _)| DataType::Decimal256(_, _))
        ),
    }
}

pub fn is_variance_support_arg_type(arg_type: &DataType) -> bool {
    matches!(
        arg_type,
        arg_type if NUMERICS.contains(arg_type)
    )
}

pub fn is_covariance_support_arg_type(arg_type: &DataType) -> bool {
    matches!(
        arg_type,
        arg_type if NUMERICS.contains(arg_type)
    )
}

pub fn is_correlation_support_arg_type(arg_type: &DataType) -> bool {
    matches!(
        arg_type,
        arg_type if NUMERICS.contains(arg_type)
    )
}

pub fn is_integer_arg_type(arg_type: &DataType) -> bool {
    arg_type.is_integer()
}

pub fn coerce_avg_type(func_name: &str, arg_types: &[DataType]) -> Result<Vec<DataType>> {
    // Supported types smallint, int, bigint, real, double precision, decimal, or interval
    // Refer to https://www.postgresql.org/docs/8.2/functions-aggregate.html doc
    fn coerced_type(func_name: &str, data_type: &DataType) -> Result<DataType> {
        match &data_type {
            DataType::Decimal128(p, s) => Ok(DataType::Decimal128(*p, *s)),
            DataType::Decimal256(p, s) => Ok(DataType::Decimal256(*p, *s)),
            d if d.is_numeric() => Ok(DataType::Float64),
            DataType::Duration(time_unit) => Ok(DataType::Duration(*time_unit)),
            DataType::Dictionary(_, v) => coerced_type(func_name, v.as_ref()),
            _ => {
                plan_err!(
                    "The function {:?} does not support inputs of type {:?}.",
                    func_name,
                    data_type
                )
            }
        }
    }
    Ok(vec![coerced_type(func_name, &arg_types[0])?])
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variance_return_data_type() -> Result<()> {
        let data_type = DataType::Float64;
        let result_type = variance_return_type(&data_type)?;
        assert_eq!(DataType::Float64, result_type);

        let data_type = DataType::Decimal128(36, 10);
        assert!(variance_return_type(&data_type).is_err());
        Ok(())
    }

    #[test]
    fn test_sum_return_data_type() -> Result<()> {
        let data_type = DataType::Decimal128(10, 5);
        let result_type = sum_return_type(&data_type)?;
        assert_eq!(DataType::Decimal128(20, 5), result_type);

        let data_type = DataType::Decimal128(36, 10);
        let result_type = sum_return_type(&data_type)?;
        assert_eq!(DataType::Decimal128(38, 10), result_type);
        Ok(())
    }

    #[test]
    fn test_covariance_return_data_type() -> Result<()> {
        let data_type = DataType::Float64;
        let result_type = covariance_return_type(&data_type)?;
        assert_eq!(DataType::Float64, result_type);

        let data_type = DataType::Decimal128(36, 10);
        assert!(covariance_return_type(&data_type).is_err());
        Ok(())
    }

    #[test]
    fn test_correlation_return_data_type() -> Result<()> {
        let data_type = DataType::Float64;
        let result_type = correlation_return_type(&data_type)?;
        assert_eq!(DataType::Float64, result_type);

        let data_type = DataType::Decimal128(36, 10);
        assert!(correlation_return_type(&data_type).is_err());
        Ok(())
    }
}
