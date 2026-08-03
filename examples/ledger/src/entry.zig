/// Declared here, allocated in store.zig, freed in close.zig. No file on its
/// own says who owns the label.
pub const Entry = struct {
    label: []const u8,
    amount: u32,
};
