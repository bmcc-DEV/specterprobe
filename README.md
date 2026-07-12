# 🔬 EAEA — Specter Probe

> *"O que este hardware faz?" em vez de "Como este hardware foi implementado?"*

**Embedded Architecture Exploration Agent** — pipeline de 9 camadas que analisa firmware de dispositivos ARM64 e produz modelos comportamentais, drivers e dispositivos de emulação, sem depender de código-fonte proprietário.

## Arquitetura

```
Firmware .zip
    ↓
┌─ Camada 1: Firmware Acquisition ─────────────────────┐
│  boot.img, super.img, DTB, ext4/erofs, ADB probe     │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 2: LLVM Reverse Core ────────────────────────┐
│  Capstone ARM64+ARM32 → CFG → LLVM IR (.ll)          │
│  PE/ELF parser, function discovery, call graph        │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 3: MMIO Discovery Engine ────────────────────┐
│  33 acessos → 2 regiões, cross-ref com DTB           │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 4: Behavioral Modeling ──────────────────────┐
│  Registradores, bitfields, state machines, polling    │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 5: Redox Driver Generator ───────────────────┐
│  Drivers Rust para Redox OS + test harness Linux      │
├──────────────────────────┬───────────────────────────┤
│  Camada 9: QEMU Device Gen  ─────────────────────────┤
│  Devices C para QEMU (SysBus, MMIO, IRQ)             │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 6: Knowledge Graph ─────────────────────────┐
│  35 nós, 67 arestas — petgraph + JSON + Neo4j CYPHER │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 7: GPU/ISP/DSP Compatibility ────────────────┐
│  Doorbell queues, ISP pipeline, DSP audio/modem       │
└──────────────────────────┬───────────────────────────┘
                           ↓
┌─ Camada 8: Device Genome ────────────────────────────┐
│  DNA YAML padronizado por dispositivo                │
└──────────────────────────────────────────────────────┘
```

## Uso Rápido

```bash
# Pipeline completo (todas as 9 camadas)
cargo run -p specter-probe -- -f Firmware.zip -o output -l -m -b -g -k -c -d -e

# Apenas drift + MMIO + comportamento
cargo run -p specter-probe -- -f output -o output -l -m -b

# Gerar drivers + devices + genoma (reusa dados existentes)
cargo run -p specter-probe -- -f output -o output -g -e -d

# Knowledge Graph + GPU/ISP/DSP
cargo run -p specter-probe -- -f output -o output -k --neo4j -c
```

## Flags

| Flag | Camada | Descrição |
|------|--------|-----------|
| `-l` | 2 | LLVM lift (ARM64 → IR) |
| `-m` | 3 | MMIO Discovery |
| `-b` | 4 | Behavioral Modeling |
| `-g` | 5 | Redox Driver Generator |
| `-k` | 6 | Knowledge Graph |
| `-c` | 7 | GPU/ISP/DSP Compatibility |
| `-d` | 8 | Device Genome (YAML) |
| `-e` | 9 | QEMU Emulator Generator |
| `--neo4j` | 6 | Export CYPHER script |

## Exemplo de Saída

```
output/
├── firmware_manifest.json    # Partições, kernel, DTB
├── lift_boot.ll              # LLVM IR (70KB)
├── lift_boot.json            # Funções, blocos, instruções
├── mmio_boot.json            # 2 regiões MMIO, 33 acessos
├── behavior_boot.json        # 2 device models c/ FSM
├── knowledge_graph.json      # 35 nós, 67 arestas
├── knowledge_graph.cql       # Script Neo4j (opcional)
├── compat_boot.json          # GPU/ISP/DSP detectados
├── compat_boot.rs            # Backend code gerado
├── genome_summary.yaml       # Sumário de genomas
├── genomes/                  # YAML por dispositivo
├── drivers/                  # Drivers Redox + test harness
└── devices/                  # Devices QEMU (C, Kconfig, meson)
```

## Licença

AGPL-3.0
