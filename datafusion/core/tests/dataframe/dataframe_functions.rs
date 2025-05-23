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

use arrow::array::{types::Int32Type, ListArray};
use arrow::datatypes::SchemaRef;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::{
    array::{Int32Array, StringArray},
    record_batch::RecordBatch,
};
use datafusion_functions_aggregate::count::count_all;
use std::sync::Arc;

use datafusion::error::Result;

use datafusion::prelude::*;
use datafusion_common::test_util::batches_to_string;
use datafusion_common::{DFSchema, ScalarValue};
use datafusion_expr::expr::Alias;
use datafusion_expr::{table_scan, ExprSchemable, LogicalPlanBuilder};
use datafusion_functions_aggregate::expr_fn::{approx_median, approx_percentile_cont};
use datafusion_functions_nested::map::map;
use insta::assert_snapshot;

fn test_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("a", DataType::Utf8, false),
        Field::new("b", DataType::Int32, false),
        Field::new("l", DataType::new_list(DataType::Int32, true), true),
    ]))
}

async fn create_test_table() -> Result<DataFrame> {
    let schema = test_schema();

    // define data.
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![
                "abcDEF",
                "abc123",
                "CBAdef",
                "123AbcDef",
            ])),
            Arc::new(Int32Array::from(vec![1, 10, 10, 100])),
            Arc::new(ListArray::from_iter_primitive::<Int32Type, _, _>(vec![
                Some(vec![Some(0), Some(1), Some(2)]),
                None,
                Some(vec![Some(3), None, Some(5)]),
                Some(vec![Some(6), Some(7)]),
            ])),
        ],
    )?;

    let ctx = SessionContext::new();

    ctx.register_batch("test", batch)?;

    ctx.table("test").await
}

/// Executes an expression on the test dataframe as a select.
async fn get_batches(expr: Expr) -> Result<Vec<RecordBatch>> {
    get_batches_with_limit(expr, 10).await
}

async fn get_batches_with_limit(expr: Expr, limit: usize) -> Result<Vec<RecordBatch>> {
    let df = create_test_table().await?;
    let df = df.select(vec![expr])?.limit(0, Some(limit))?;
    df.collect().await
}

#[tokio::test]
async fn test_fn_ascii() -> Result<()> {
    let expr = ascii(col("a"));

    let batches = get_batches_with_limit(expr, 1).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +---------------+
    | ascii(test.a) |
    +---------------+
    | 97            |
    +---------------+
    "
    );

    Ok(())
}

#[tokio::test]
async fn test_fn_bit_length() -> Result<()> {
    let expr = bit_length(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
    batches_to_string(&batches),
    @r"
    +--------------------+
    | bit_length(test.a) |
    +--------------------+
    | 48                 |
    | 48                 |
    | 48                 |
    | 72                 |
    +--------------------+
    "
    );

    Ok(())
}

#[tokio::test]
async fn test_fn_btrim() -> Result<()> {
    let expr = btrim(vec![lit("      a b c             ")]);

    let batches = get_batches_with_limit(expr, 1).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-----------------------------------------+
    | btrim(Utf8("      a b c             ")) |
    +-----------------------------------------+
    | a b c                                   |
    +-----------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_btrim_with_chars() -> Result<()> {
    let expr = btrim(vec![col("a"), lit("ab")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +--------------------------+
    | btrim(test.a,Utf8("ab")) |
    +--------------------------+
    | cDEF                     |
    | c123                     |
    | CBAdef                   |
    | 123AbcDef                |
    +--------------------------+
    "#
    );

    Ok(())
}

#[tokio::test]
async fn test_fn_nullif() -> Result<()> {
    let expr = nullif(col("a"), lit("abcDEF"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-------------------------------+
    | nullif(test.a,Utf8("abcDEF")) |
    +-------------------------------+
    |                               |
    | abc123                        |
    | CBAdef                        |
    | 123AbcDef                     |
    +-------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_arrow_cast() -> Result<()> {
    let expr = arrow_typeof(arrow_cast(col("b"), lit("Float64")));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +--------------------------------------------------+
    | arrow_typeof(arrow_cast(test.b,Utf8("Float64"))) |
    +--------------------------------------------------+
    | Float64                                          |
    | Float64                                          |
    | Float64                                          |
    | Float64                                          |
    +--------------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_nvl() -> Result<()> {
    let lit_null = lit(ScalarValue::Utf8(None));
    // nvl(CASE WHEN a = 'abcDEF' THEN NULL ELSE a END, 'TURNED_NULL')
    let expr = nvl(
        when(col("a").eq(lit("abcDEF")), lit_null)
            .otherwise(col("a"))
            .unwrap(),
        lit("TURNED_NULL"),
    )
    .alias("nvl_expr");

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-------------+
    | nvl_expr    |
    +-------------+
    | TURNED_NULL |
    | abc123      |
    | CBAdef      |
    | 123AbcDef   |
    +-------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_nvl2() -> Result<()> {
    let lit_null = lit(ScalarValue::Utf8(None));
    // nvl2(CASE WHEN a = 'abcDEF' THEN NULL ELSE a END, 'NON_NUll', 'TURNED_NULL')
    let expr = nvl2(
        when(col("a").eq(lit("abcDEF")), lit_null)
            .otherwise(col("a"))
            .unwrap(),
        lit("NON_NULL"),
        lit("TURNED_NULL"),
    )
    .alias("nvl2_expr");

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-------------+
    | nvl2_expr   |
    +-------------+
    | TURNED_NULL |
    | NON_NULL    |
    | NON_NULL    |
    | NON_NULL    |
    +-------------+
    ");

    Ok(())
}
#[tokio::test]
async fn test_fn_arrow_typeof() -> Result<()> {
    let expr = arrow_typeof(col("l"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +------------------------------------------------------------------------------------------------------------------+
    | arrow_typeof(test.l)                                                                                             |
    +------------------------------------------------------------------------------------------------------------------+
    | List(Field { name: "item", data_type: Int32, nullable: true, dict_id: 0, dict_is_ordered: false, metadata: {} }) |
    | List(Field { name: "item", data_type: Int32, nullable: true, dict_id: 0, dict_is_ordered: false, metadata: {} }) |
    | List(Field { name: "item", data_type: Int32, nullable: true, dict_id: 0, dict_is_ordered: false, metadata: {} }) |
    | List(Field { name: "item", data_type: Int32, nullable: true, dict_id: 0, dict_is_ordered: false, metadata: {} }) |
    +------------------------------------------------------------------------------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_struct() -> Result<()> {
    let expr = r#struct(vec![col("a"), col("b")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +--------------------------+
    | struct(test.a,test.b)    |
    +--------------------------+
    | {c0: abcDEF, c1: 1}      |
    | {c0: abc123, c1: 10}     |
    | {c0: CBAdef, c1: 10}     |
    | {c0: 123AbcDef, c1: 100} |
    +--------------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_named_struct() -> Result<()> {
    let expr = named_struct(vec![lit("column_a"), col("a"), lit("column_b"), col("b")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +---------------------------------------------------------------+
    | named_struct(Utf8("column_a"),test.a,Utf8("column_b"),test.b) |
    +---------------------------------------------------------------+
    | {column_a: abcDEF, column_b: 1}                               |
    | {column_a: abc123, column_b: 10}                              |
    | {column_a: CBAdef, column_b: 10}                              |
    | {column_a: 123AbcDef, column_b: 100}                          |
    +---------------------------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_coalesce() -> Result<()> {
    let expr = coalesce(vec![lit(ScalarValue::Utf8(None)), lit("ab")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +---------------------------------+
    | coalesce(Utf8(NULL),Utf8("ab")) |
    +---------------------------------+
    | ab                              |
    | ab                              |
    | ab                              |
    | ab                              |
    +---------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_approx_median() -> Result<()> {
    let expr = approx_median(col("b"));

    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-----------------------+
    | approx_median(test.b) |
    +-----------------------+
    | 10                    |
    +-----------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_approx_percentile_cont() -> Result<()> {
    let expr = approx_percentile_cont(col("b").sort(true, false), lit(0.5), None);

    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +---------------------------------------------------------------------------+
    | approx_percentile_cont(Float64(0.5)) WITHIN GROUP [test.b ASC NULLS LAST] |
    +---------------------------------------------------------------------------+
    | 10                                                                        |
    +---------------------------------------------------------------------------+
    ");

    let expr = approx_percentile_cont(col("b").sort(false, false), lit(0.1), None);

    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +----------------------------------------------------------------------------+
    | approx_percentile_cont(Float64(0.1)) WITHIN GROUP [test.b DESC NULLS LAST] |
    +----------------------------------------------------------------------------+
    | 100                                                                        |
    +----------------------------------------------------------------------------+
    ");

    // the arg2 parameter is a complex expr, but it can be evaluated to the literal value
    let alias_expr = Expr::Alias(Alias::new(
        cast(lit(0.5), DataType::Float32),
        None::<&str>,
        "arg_2".to_string(),
    ));
    let expr = approx_percentile_cont(col("b").sort(true, false), alias_expr, None);
    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +--------------------------------------------------------------------+
    | approx_percentile_cont(arg_2) WITHIN GROUP [test.b ASC NULLS LAST] |
    +--------------------------------------------------------------------+
    | 10                                                                 |
    +--------------------------------------------------------------------+
    "
    );

    let alias_expr = Expr::Alias(Alias::new(
        cast(lit(0.1), DataType::Float32),
        None::<&str>,
        "arg_2".to_string(),
    ));
    let expr = approx_percentile_cont(col("b").sort(false, false), alias_expr, None);
    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +---------------------------------------------------------------------+
    | approx_percentile_cont(arg_2) WITHIN GROUP [test.b DESC NULLS LAST] |
    +---------------------------------------------------------------------+
    | 100                                                                 |
    +---------------------------------------------------------------------+
    "
    );

    // with number of centroids set
    let expr = approx_percentile_cont(col("b").sort(true, false), lit(0.5), Some(lit(2)));

    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +------------------------------------------------------------------------------------+
    | approx_percentile_cont(Float64(0.5),Int32(2)) WITHIN GROUP [test.b ASC NULLS LAST] |
    +------------------------------------------------------------------------------------+
    | 30                                                                                 |
    +------------------------------------------------------------------------------------+
    ");

    let expr =
        approx_percentile_cont(col("b").sort(false, false), lit(0.1), Some(lit(2)));

    let df = create_test_table().await?;
    let batches = df.aggregate(vec![], vec![expr]).unwrap().collect().await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-------------------------------------------------------------------------------------+
    | approx_percentile_cont(Float64(0.1),Int32(2)) WITHIN GROUP [test.b DESC NULLS LAST] |
    +-------------------------------------------------------------------------------------+
    | 69                                                                                  |
    +-------------------------------------------------------------------------------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_character_length() -> Result<()> {
    let expr = character_length(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +--------------------------+
    | character_length(test.a) |
    +--------------------------+
    | 6                        |
    | 6                        |
    | 6                        |
    | 9                        |
    +--------------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_chr() -> Result<()> {
    let expr = chr(lit(128175));

    let batches = get_batches_with_limit(expr, 1).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +--------------------+
    | chr(Int32(128175)) |
    +--------------------+
    | 💯                 |
    +--------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_initcap() -> Result<()> {
    let expr = initcap(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-----------------+
    | initcap(test.a) |
    +-----------------+
    | Abcdef          |
    | Abc123          |
    | Cbadef          |
    | 123abcdef       |
    +-----------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_left() -> Result<()> {
    let expr = left(col("a"), lit(3));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-----------------------+
    | left(test.a,Int32(3)) |
    +-----------------------+
    | abc                   |
    | abc                   |
    | CBA                   |
    | 123                   |
    +-----------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_lower() -> Result<()> {
    let expr = lower(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +---------------+
    | lower(test.a) |
    +---------------+
    | abcdef        |
    | abc123        |
    | cbadef        |
    | 123abcdef     |
    +---------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_lpad() -> Result<()> {
    let expr = lpad(vec![col("a"), lit(10)]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +------------------------+
    | lpad(test.a,Int32(10)) |
    +------------------------+
    |     abcDEF             |
    |     abc123             |
    |     CBAdef             |
    |  123AbcDef             |
    +------------------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_lpad_with_string() -> Result<()> {
    let expr = lpad(vec![col("a"), lit(10), lit("*")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +----------------------------------+
    | lpad(test.a,Int32(10),Utf8("*")) |
    +----------------------------------+
    | ****abcDEF                       |
    | ****abc123                       |
    | ****CBAdef                       |
    | *123AbcDef                       |
    +----------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_ltrim() -> Result<()> {
    let expr = ltrim(vec![lit("      a b c             ")]);

    let batches = get_batches_with_limit(expr, 1).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-----------------------------------------+
    | ltrim(Utf8("      a b c             ")) |
    +-----------------------------------------+
    | a b c                                   |
    +-----------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_ltrim_with_columns() -> Result<()> {
    let expr = ltrim(vec![col("a")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +---------------+
    | ltrim(test.a) |
    +---------------+
    | abcDEF        |
    | abc123        |
    | CBAdef        |
    | 123AbcDef     |
    +---------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_md5() -> Result<()> {
    let expr = md5(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +----------------------------------+
    | md5(test.a)                      |
    +----------------------------------+
    | ea2de8bd80f3a1f52c754214fc9b0ed1 |
    | e99a18c428cb38d5f260853678922e03 |
    | 11ed4a6e9985df40913eead67f022e27 |
    | 8f5e60e523c9253e623ae38bb58c399a |
    +----------------------------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_regexp_like() -> Result<()> {
    let expr = regexp_like(col("a"), lit("[a-z]"), None);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-----------------------------------+
    | regexp_like(test.a,Utf8("[a-z]")) |
    +-----------------------------------+
    | true                              |
    | true                              |
    | true                              |
    | true                              |
    +-----------------------------------+
    "#);

    let expr = regexp_like(col("a"), lit("abc"), Some(lit("i")));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-------------------------------------------+
    | regexp_like(test.a,Utf8("abc"),Utf8("i")) |
    +-------------------------------------------+
    | true                                      |
    | true                                      |
    | false                                     |
    | true                                      |
    +-------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_regexp_match() -> Result<()> {
    let expr = regexp_match(col("a"), lit("[a-z]"), None);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +------------------------------------+
    | regexp_match(test.a,Utf8("[a-z]")) |
    +------------------------------------+
    | [a]                                |
    | [a]                                |
    | [d]                                |
    | [b]                                |
    +------------------------------------+
    "#);

    let expr = regexp_match(col("a"), lit("[A-Z]"), Some(lit("i")));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +----------------------------------------------+
    | regexp_match(test.a,Utf8("[A-Z]"),Utf8("i")) |
    +----------------------------------------------+
    | [a]                                          |
    | [a]                                          |
    | [C]                                          |
    | [A]                                          |
    +----------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_regexp_replace() -> Result<()> {
    let expr = regexp_replace(col("a"), lit("[a-z]"), lit("x"), Some(lit("g")));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +----------------------------------------------------------+
    | regexp_replace(test.a,Utf8("[a-z]"),Utf8("x"),Utf8("g")) |
    +----------------------------------------------------------+
    | xxxDEF                                                   |
    | xxx123                                                   |
    | CBAxxx                                                   |
    | 123AxxDxx                                                |
    +----------------------------------------------------------+
    "#);

    let expr = regexp_replace(col("a"), lit("[a-z]"), lit("x"), None);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +------------------------------------------------+
    | regexp_replace(test.a,Utf8("[a-z]"),Utf8("x")) |
    +------------------------------------------------+
    | xbcDEF                                         |
    | xbc123                                         |
    | CBAxef                                         |
    | 123AxcDef                                      |
    +------------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_replace() -> Result<()> {
    let expr = replace(col("a"), lit("abc"), lit("x"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +---------------------------------------+
    | replace(test.a,Utf8("abc"),Utf8("x")) |
    +---------------------------------------+
    | xDEF                                  |
    | x123                                  |
    | CBAdef                                |
    | 123AbcDef                             |
    +---------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_repeat() -> Result<()> {
    let expr = repeat(col("a"), lit(2));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-------------------------+
    | repeat(test.a,Int32(2)) |
    +-------------------------+
    | abcDEFabcDEF            |
    | abc123abc123            |
    | CBAdefCBAdef            |
    | 123AbcDef123AbcDef      |
    +-------------------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_reverse() -> Result<()> {
    let expr = reverse(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-----------------+
    | reverse(test.a) |
    +-----------------+
    | FEDcba          |
    | 321cba          |
    | fedABC          |
    | feDcbA321       |
    +-----------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_right() -> Result<()> {
    let expr = right(col("a"), lit(3));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +------------------------+
    | right(test.a,Int32(3)) |
    +------------------------+
    | DEF                    |
    | 123                    |
    | def                    |
    | Def                    |
    +------------------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_rpad() -> Result<()> {
    let expr = rpad(vec![col("a"), lit(11)]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +------------------------+
    | rpad(test.a,Int32(11)) |
    +------------------------+
    | abcDEF                 |
    | abc123                 |
    | CBAdef                 |
    | 123AbcDef              |
    +------------------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_rpad_with_characters() -> Result<()> {
    let expr = rpad(vec![col("a"), lit(11), lit("x")]);

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +----------------------------------+
    | rpad(test.a,Int32(11),Utf8("x")) |
    +----------------------------------+
    | abcDEFxxxxx                      |
    | abc123xxxxx                      |
    | CBAdefxxxxx                      |
    | 123AbcDefxx                      |
    +----------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_sha224() -> Result<()> {
    let expr = sha224(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +----------------------------------------------------------+
    | sha224(test.a)                                           |
    +----------------------------------------------------------+
    | 8b9ef961d2b19cfe7ee2a8452e3adeea98c7b22954b4073976bf80ee |
    | 5c69bb695cc29b93d655e1a4bb5656cda624080d686f74477ea09349 |
    | b3b3783b7470594e7ddb845eca0aec5270746dd6d0bc309bb948ceab |
    | fc8a30d59386d78053328440c6670c3b583404a905cbe9bbd491a517 |
    +----------------------------------------------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_split_part() -> Result<()> {
    let expr = split_part(col("a"), lit("b"), lit(1));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +---------------------------------------+
    | split_part(test.a,Utf8("b"),Int32(1)) |
    +---------------------------------------+
    | a                                     |
    | a                                     |
    | CBAdef                                |
    | 123A                                  |
    +---------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_starts_with() -> Result<()> {
    let expr = starts_with(col("a"), lit("abc"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +---------------------------------+
    | starts_with(test.a,Utf8("abc")) |
    +---------------------------------+
    | true                            |
    | true                            |
    | false                           |
    | false                           |
    +---------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_ends_with() -> Result<()> {
    let expr = ends_with(col("a"), lit("DEF"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-------------------------------+
    | ends_with(test.a,Utf8("DEF")) |
    +-------------------------------+
    | true                          |
    | false                         |
    | false                         |
    | false                         |
    +-------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_strpos() -> Result<()> {
    let expr = strpos(col("a"), lit("f"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +--------------------------+
    | strpos(test.a,Utf8("f")) |
    +--------------------------+
    | 0                        |
    | 0                        |
    | 6                        |
    | 9                        |
    +--------------------------+
    "#);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_substr() -> Result<()> {
    let expr = substr(col("a"), lit(2));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +-------------------------+
    | substr(test.a,Int32(2)) |
    +-------------------------+
    | bcDEF                   |
    | bc123                   |
    | BAdef                   |
    | 23AbcDef                |
    +-------------------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_cast() -> Result<()> {
    let expr = cast(col("b"), DataType::Float64);
    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +--------+
    | test.b |
    +--------+
    | 1.0    |
    | 10.0   |
    | 10.0   |
    | 100.0  |
    +--------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_to_hex() -> Result<()> {
    let expr = to_hex(col("b"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +----------------+
    | to_hex(test.b) |
    +----------------+
    | 1              |
    | a              |
    | a              |
    | 64             |
    +----------------+
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "unicode_expressions")]
async fn test_fn_translate() -> Result<()> {
    let expr = translate(col("a"), lit("bc"), lit("xx"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-----------------------------------------+
    | translate(test.a,Utf8("bc"),Utf8("xx")) |
    +-----------------------------------------+
    | axxDEF                                  |
    | axx123                                  |
    | CBAdef                                  |
    | 123AxxDef                               |
    +-----------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_upper() -> Result<()> {
    let expr = upper(col("a"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r"
    +---------------+
    | upper(test.a) |
    +---------------+
    | ABCDEF        |
    | ABC123        |
    | CBADEF        |
    | 123ABCDEF     |
    +---------------+
    ");

    Ok(())
}

#[tokio::test]
async fn test_fn_encode() -> Result<()> {
    let expr = encode(col("a"), lit("hex"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +----------------------------+
    | encode(test.a,Utf8("hex")) |
    +----------------------------+
    | 616263444546               |
    | 616263313233               |
    | 434241646566               |
    | 313233416263446566         |
    +----------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_decode() -> Result<()> {
    // Note that the decode function returns binary, and the default display of
    // binary is "hexadecimal" and therefore the output looks like decode did
    // nothing. So compare to a constant.
    let df_schema = DFSchema::try_from(test_schema().as_ref().clone())?;
    let expr = decode(encode(col("a"), lit("hex")), lit("hex"))
        // need to cast to utf8 otherwise the default display of binary array is hex
        // so it looks like nothing is done
        .cast_to(&DataType::Utf8, &df_schema)?;

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +------------------------------------------------+
    | decode(encode(test.a,Utf8("hex")),Utf8("hex")) |
    +------------------------------------------------+
    | abcDEF                                         |
    | abc123                                         |
    | CBAdef                                         |
    | 123AbcDef                                      |
    +------------------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_array_to_string() -> Result<()> {
    let expr = array_to_string(col("l"), lit("***"));

    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +-------------------------------------+
    | array_to_string(test.l,Utf8("***")) |
    +-------------------------------------+
    | 0***1***2                           |
    |                                     |
    | 3***5                               |
    | 6***7                               |
    +-------------------------------------+
    "#);

    Ok(())
}

#[tokio::test]
async fn test_fn_map() -> Result<()> {
    let expr = map(
        vec![lit("a"), lit("b"), lit("c")],
        vec![lit(1), lit(2), lit(3)],
    );
    let batches = get_batches(expr).await?;

    assert_snapshot!(
        batches_to_string(&batches),
        @r#"
    +---------------------------------------------------------------------------------------+
    | map(make_array(Utf8("a"),Utf8("b"),Utf8("c")),make_array(Int32(1),Int32(2),Int32(3))) |
    +---------------------------------------------------------------------------------------+
    | {a: 1, b: 2, c: 3}                                                                    |
    | {a: 1, b: 2, c: 3}                                                                    |
    | {a: 1, b: 2, c: 3}                                                                    |
    | {a: 1, b: 2, c: 3}                                                                    |
    +---------------------------------------------------------------------------------------+
    "#);

    Ok(())
}

/// Call count wildcard from dataframe API
#[tokio::test]
async fn test_count_wildcard() -> Result<()> {
    let schema = Schema::new(vec![
        Field::new("a", DataType::UInt32, false),
        Field::new("b", DataType::UInt32, false),
        Field::new("c", DataType::UInt32, false),
    ]);

    let table_scan = table_scan(Some("test"), &schema, None)?.build()?;
    let plan = LogicalPlanBuilder::from(table_scan)
        .aggregate(vec![col("b")], vec![count_all()])
        .unwrap()
        .project(vec![count_all()])
        .unwrap()
        .sort(vec![count_all().sort(true, false)])
        .unwrap()
        .build()
        .unwrap();

    let formatted_plan = plan.display_indent_schema().to_string();
    assert_snapshot!(formatted_plan,
        @r"
    Sort: count(*) ASC NULLS LAST [count(*):Int64]
      Projection: count(*) [count(*):Int64]
        Aggregate: groupBy=[[test.b]], aggr=[[count(Int64(1)) AS count(*)]] [b:UInt32, count(*):Int64]
          TableScan: test [a:UInt32, b:UInt32, c:UInt32]
    ");

    Ok(())
}
