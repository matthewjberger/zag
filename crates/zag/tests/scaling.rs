//! Times the passes over a synthetic program, because a design that claims to
//! work on a whole codebase should be able to say what it costs on one.
//!
//! `ZAG_SCALE` sets how many structs to generate. The default keeps this a
//! normal test; a larger value turns it into the measurement `just bench` runs.

use std::time::Instant;
use zag_facts::build::{
    declare_field, declare_function, declare_parameter, declare_struct, intern,
    push_allocator_source, push_call, push_call_argument, push_field_assignment, push_integer_type,
    push_memory_operation, push_opaque_type, push_slice_type, set_struct_deinit,
};
use zag_facts::tables::{
    AllocatorSourceKind, AssignmentSource, MemoryOperationKind, PARAMETER_FLAG_ALLOCATOR,
    PlaceKind, Tables, empty_tables, field_count, function_count,
};
use zag_facts::{FunctionId, StructId};

const FIELDS_PER_STRUCT: usize = 6;

/// One struct per iteration, each with an owned field allocated in its own
/// `init` and freed one call away, which is the shape the analysis works
/// hardest on.
fn synthetic(structs: usize) -> Tables {
    let mut tables = empty_tables();
    tables.target = intern(&mut tables.strings, b"x86_64-linux");
    let byte = push_integer_type(&mut tables, 8, false);
    let word = push_integer_type(&mut tables, 32, false);
    let slice = push_slice_type(&mut tables, byte);
    let allocator_name = intern(&mut tables.strings, b"Allocator");
    let allocator = push_opaque_type(&mut tables, allocator_name);

    let global = push_allocator_source(
        &mut tables,
        AllocatorSourceKind::Global,
        FunctionId(zag_facts::NO_INDEX),
        zag_facts::NO_INDEX,
    );

    let mut owned_fields = Vec::with_capacity(structs);
    for index in 0..structs {
        let owner = declare_struct(&mut tables, format!("Type{index}").as_bytes(), 64, 8, 0);
        owned_fields.push(declare_field(&mut tables, owner, b"data", slice, 0));
        for field in 1..FIELDS_PER_STRUCT {
            declare_field(
                &mut tables,
                owner,
                format!("scalar{field}").as_bytes(),
                word,
                (16 + field * 4) as u32,
            );
        }
    }

    for index in 0..structs {
        let owner = StructId(index as u32);
        let initialize = declare_function(&mut tables, format!("init{index}").as_bytes(), owner);
        declare_parameter(
            &mut tables,
            initialize,
            b"allocator",
            allocator,
            PARAMETER_FLAG_ALLOCATOR,
        );
        let deinitialize =
            declare_function(&mut tables, format!("deinit{index}").as_bytes(), owner);
        declare_parameter(
            &mut tables,
            deinitialize,
            b"allocator",
            allocator,
            PARAMETER_FLAG_ALLOCATOR,
        );
        set_struct_deinit(&mut tables, owner, deinitialize);
        let release = declare_function(&mut tables, format!("release{index}").as_bytes(), owner);
        declare_parameter(
            &mut tables,
            release,
            b"allocator",
            allocator,
            PARAMETER_FLAG_ALLOCATOR,
        );
    }

    let root = declare_function(&mut tables, b"root", StructId(zag_facts::NO_INDEX));

    for (index, owned) in owned_fields.iter().enumerate() {
        let initialize = FunctionId((index * 3) as u32);
        let deinitialize = FunctionId((index * 3 + 1) as u32);
        let release = FunctionId((index * 3 + 2) as u32);
        let initialize_allocator =
            push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, initialize, 0);
        let release_allocator =
            push_allocator_source(&mut tables, AllocatorSourceKind::Parameter, release, 0);

        let call = push_call(&mut tables, initialize, release);
        push_call_argument(&mut tables, call, 0, global);
        let call = push_call(&mut tables, deinitialize, release);
        push_call_argument(&mut tables, call, 0, global);

        let allocate = push_memory_operation(
            &mut tables,
            initialize,
            MemoryOperationKind::Allocate,
            initialize_allocator,
            PlaceKind::FieldOfParameter,
            *owned,
        );
        push_memory_operation(
            &mut tables,
            release,
            MemoryOperationKind::Free,
            release_allocator,
            PlaceKind::FieldOfParameter,
            *owned,
        );
        push_field_assignment(
            &mut tables,
            *owned,
            initialize,
            AssignmentSource::Allocation,
            allocate,
        );
    }

    // One caller for every init, which is what pins each allocator to the
    // global one, and a fan out wide enough to be worth walking.
    for index in 0..structs {
        let call = push_call(&mut tables, root, FunctionId((index * 3) as u32));
        push_call_argument(&mut tables, call, 0, global);
    }
    tables
}

fn scale() -> usize {
    std::env::var("ZAG_SCALE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200)
}

#[test]
fn the_passes_scale_to_a_whole_program() {
    let structs = scale();
    let built = Instant::now();
    let tables = synthetic(structs);
    let build_time = built.elapsed();

    let validated = Instant::now();
    zag_facts::validate::validate(&tables).expect("the generator builds valid tables");
    let validate_time = validated.elapsed();

    let analysed = Instant::now();
    let analysis = zag_analysis::analyze(&tables);
    let analyse_time = analysed.elapsed();

    let emitted = Instant::now();
    let output = zag_emit::generate(&tables, &analysis).expect("the tree prints");
    let emit_time = emitted.elapsed();

    let encoded = Instant::now();
    let bytes = zag_facts::wire::encode(&tables);
    let encode_time = encoded.elapsed();

    println!(
        "structs={structs} fields={} functions={} facts={}KiB rust={}KiB",
        field_count(&tables.fields),
        function_count(&tables.functions),
        bytes.len() / 1024,
        output.source.len() / 1024,
    );
    println!(
        "build={build_time:?} validate={validate_time:?} analyse={analyse_time:?} emit={emit_time:?} encode={encode_time:?}"
    );

    assert!(analysis.provenance.converged);
    assert_eq!(analysis.ownership.class.len(), field_count(&tables.fields));
    let owned = analysis
        .ownership
        .class
        .iter()
        .filter(|class| **class == zag_analysis::ownership::OwnershipClass::Owned)
        .count();
    assert_eq!(owned, structs, "every generated struct owns its data field");
}
