use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arch {
    Arm64,
    Arm32,
    Thumb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Size {
    B8,
    B16,
    B32,
    B64,
    B128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reg {
    X(u8),
    W(u8),
    R(u8),
    Sp,
    Pc,
    Lr,
    Fp,
    Xzr,
    Wzr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftType {
    Lsl,
    Lsr,
    Asr,
    Ror,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtType {
    Uxtb,
    Uxth,
    Uxtw,
    Uxtx,
    Sxtb,
    Sxth,
    Sxtw,
    Sxtx,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemAddr {
    pub base: Reg,
    pub offset: i64,
    pub post_index: bool,
    pub pre_index: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmOperand {
    Reg(Reg),
    Imm(i64),
    Mem(MemAddr),
    ShiftReg(Reg, ShiftType, u8),
    ExtendedReg(Reg, ExtType, Option<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CondCode {
    Eq, Ne, Cs, Cc, Mi, Pl, Vs, Vc,
    Hi, Ls, Ge, Lt, Gt, Le, Al, Nv,
}

impl CondCode {
    pub fn from_arm64(cc: u32) -> Self {
        match cc {
            0 => CondCode::Eq,  1 => CondCode::Ne,
            2 => CondCode::Cs,  3 => CondCode::Cc,
            4 => CondCode::Mi,  5 => CondCode::Pl,
            6 => CondCode::Vs,  7 => CondCode::Vc,
            8 => CondCode::Hi,  9 => CondCode::Ls,
            10 => CondCode::Ge, 11 => CondCode::Lt,
            12 => CondCode::Gt, 13 => CondCode::Le,
            14 => CondCode::Al, _ => CondCode::Nv,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstKind {
    Mov(Size, Reg, ArmOperand),
    Add(Size, Reg, Reg, ArmOperand),
    Sub(Size, Reg, Reg, ArmOperand),
    Cmp(Size, Reg, ArmOperand),
    Mul(Size, Reg, Reg, ArmOperand),
    Sdiv(Size, Reg, Reg, ArmOperand),
    Udiv(Size, Reg, Reg, ArmOperand),
    And_(Size, Reg, Reg, ArmOperand),
    Orr(Size, Reg, Reg, ArmOperand),
    Eor(Size, Reg, Reg, ArmOperand),
    Lsl(Size, Reg, Reg, ArmOperand),
    Lsr(Size, Reg, Reg, ArmOperand),
    Asr(Size, Reg, Reg, ArmOperand),

    Load(Size, Reg, MemAddr),
    Store(Size, MemAddr, Reg),
    LoadPair(Size, Reg, Reg, MemAddr),
    StorePair(Size, MemAddr, Reg, Reg),
    LoadLiteral(Size, Reg, u64),

    Adrp(Reg, u64),
    Adr(Reg, u64),

    Branch(CondCode, u64),
    BranchAlways(u64),
    BranchLink(u64),
    BranchReg(Reg),
    CompareBranch(bool, Reg, u64), // cbz/cbnz

    Svc(u32),

    Nop,
    Ret(Option<Reg>),
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub op_str: String,
    pub kind: InstKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicBlock {
    pub address: u64,
    pub instructions: Vec<Instruction>,
    pub successors: Vec<u64>,
    pub cond_successor: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub entry: u64,
    pub name: String,
    pub blocks: Vec<BasicBlock>,
    pub exit_blocks: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftOutput {
    pub arch: Arch,
    pub functions: Vec<Function>,
    pub total_instructions: usize,
    pub lifted_functions: usize,
    pub ir_text: String,
    pub entry_point: u64,
}
