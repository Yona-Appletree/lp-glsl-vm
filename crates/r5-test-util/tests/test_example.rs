//! Example test using R5FnTest.

use r5_builder::FunctionBuilder;
use r5_ir::{Signature, Type};
use r5_test_util::R5FnTest;

#[test]
fn test_add_function() {
    // Build IR: fn add(a: i32, b: i32) -> i32 { a + b }
    let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
    let mut builder = FunctionBuilder::new(sig);

    // Create entry block with parameters (a and b)
    let a = builder.new_value();
    let b = builder.new_value();
    let block_idx = builder.create_block_with_params(vec![a, b]);

    let result = builder.new_value();

    {
        let mut block_builder = builder.block_builder(block_idx);
        block_builder.iadd(result, a, b);
        block_builder.return_(&vec![result]);
    }

    let func = builder.finish();

    // Test the function
    R5FnTest::new(func)
        .with_args(&[5, 10])
        .expect_return(15)
        .run();
}
