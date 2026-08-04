pub const NO_INDEX: u32 = u32::MAX;

macro_rules! handle {
    ($name:ident) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $name(pub u32);
    };
}

handle!(StringId);
handle!(ModuleId);
handle!(ArtifactId);
handle!(TypeId);
handle!(StructId);
handle!(FieldId);
handle!(FunctionId);
handle!(ParameterId);
handle!(CallId);
handle!(AllocatorSourceId);
handle!(MemoryOperationId);
handle!(ExpressionId);
