#[cfg(test)]
use rspirv::{
    binary::{Assemble, Disassemble},
    dr::Operand,
    spirv::{
        AddressingModel, Decoration, ExecutionModel, FunctionControl, MemoryModel, StorageClass,
    },
};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use vulkano::shader::spirv::Spirv;

#[test]
fn push_uint_before_struct() {
    let mut b = rspirv::dr::Builder::new();
    b.memory_model(AddressingModel::Logical, MemoryModel::Vulkan);
    let void = b.type_void();
    let voidf = b.type_function(void, vec![void]);
    let uint = b.type_int(32, 0);
    let ptr_push_uint = b.type_pointer(None, StorageClass::PushConstant, uint);
    let zero = b.constant_u32(uint, 0);
    let push_struct = b.type_struct([uint, uint]);
    b.member_decorate(
        push_struct,
        0,
        Decoration::Offset,
        [Operand::LiteralInt32(0)],
    );
    b.member_decorate(
        push_struct,
        1,
        Decoration::Offset,
        [Operand::LiteralInt32(4)],
    );
    let ptr_push_struct = b.type_pointer(None, StorageClass::PushConstant, push_struct);
    let var1 = b.variable(ptr_push_struct, None, StorageClass::PushConstant, None);
    let func1 = b
        .begin_function(
            void,
            None,
            FunctionControl::DONT_INLINE | FunctionControl::CONST,
            voidf,
        )
        .unwrap();
    b.begin_block(None).unwrap();
    let var1_0 = b.access_chain(ptr_push_uint, None, var1, [zero]).unwrap();
    b.load(uint, None, var1_0, None, []).unwrap();
    b.ret().unwrap();
    b.end_function().unwrap();
    b.entry_point(ExecutionModel::GLCompute, func1, "main1", vec![]);
    let module = b.module();
    eprintln!("{}", module.disassemble());
    let spirv = Spirv::new(&module.assemble()).unwrap();
    dbg!(&spirv);
    let entry_points: HashMap<_, _> = vulkano::shader::reflect::entry_points(&spirv).collect();
    dbg!(entry_points);
}

#[test]
fn multiple_entry_points() {
    use vulkano::shader::spirv::Spirv;

    let mut b = rspirv::dr::Builder::new();
    b.memory_model(AddressingModel::Logical, MemoryModel::Vulkan);
    let void = b.type_void();
    let voidf = b.type_function(void, vec![void]);
    let uint = b.type_int(32, 0);
    let zero = b.constant_u32(uint, 0);
    let push_struct = b.type_struct([uint, uint]);
    b.member_decorate(
        push_struct,
        0,
        Decoration::Offset,
        [Operand::LiteralInt32(0)],
    );
    b.member_decorate(
        push_struct,
        1,
        Decoration::Offset,
        [Operand::LiteralInt32(4)],
    );
    let ptr_push_struct = b.type_pointer(None, StorageClass::PushConstant, push_struct);
    let ptr_push_uint = b.type_pointer(None, StorageClass::PushConstant, uint);
    let var1 = b.variable(ptr_push_struct, None, StorageClass::PushConstant, None);
    let func1 = b
        .begin_function(void, None, FunctionControl::NONE, voidf)
        .unwrap();
    b.begin_block(None).unwrap();
    let var1_0 = b.access_chain(ptr_push_uint, None, var1, [zero]).unwrap();
    b.load(uint, None, var1_0, None, []).unwrap();
    b.ret().unwrap();
    b.end_function().unwrap();
    b.entry_point(ExecutionModel::GLCompute, func1, "main1", vec![]);
    let func2 = b
        .begin_function(void, None, FunctionControl::NONE, voidf)
        .unwrap();
    b.begin_block(None).unwrap();
    b.ret().unwrap();
    b.end_function().unwrap();
    b.entry_point(ExecutionModel::GLCompute, func2, "main2", vec![]);
    let module = b.module();
    println!("{}", module.disassemble());
    let spirv = Spirv::new(&module.assemble()).unwrap();
    dbg!(&spirv);
    let entry_points: HashMap<_, _> = vulkano::shader::reflect::entry_points(&spirv).collect();
    dbg!(&entry_points);
    assert_eq!(
        entry_points
            .values()
            .find(|x| x.name == "main1")
            .unwrap()
            .push_constant_requirements
            .unwrap()
            .size,
        8
    );
    assert_eq!(
        entry_points
            .values()
            .find(|x| x.name == "main2")
            .unwrap()
            .push_constant_requirements,
        None
    );
}
