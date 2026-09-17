import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Outcom } from "../target/types/outcom";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";

function must(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`Missing env ${name}`);
  return v;
}

function parse32Bytes(label: string, raw: string): number[] {
  const hex = raw.startsWith("0x") ? raw.slice(2) : raw;
  if (hex.length !== 64) {
    throw new Error(`${label} must be 32 bytes hex (64 chars), got ${hex.length}`);
  }
  return Array.from(Buffer.from(hex, "hex"));
}

function loadKeypair(path: string): Keypair {
  const secret = JSON.parse(fs.readFileSync(path, "utf8"));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Outcom as Program<Outcom>;
  const admin = provider.wallet;

  // MOCK=true → endpoint is a keypair you control and later sign lz_receive with.
  // MOCK=false → ENDPOINT_PROGRAM is the official LZ Solana endpoint pubkey.
  const mock = String(process.env.MOCK ?? "true") === "true";

  const srcEid = Number(must("SRC_EID"));
  const peerBytes = parse32Bytes("PEER_ADDRESS", must("PEER_ADDRESS"));

  let endpointProgram: PublicKey;
  if (mock) {
    const kp = loadKeypair(must("MOCK_ENDPOINT_KEYPAIR"));
    endpointProgram = kp.publicKey;
    console.log("MOCK endpoint signer:", endpointProgram.toBase58());
    console.log("This same keypair must sign lz_receive as endpointProgram.");
  } else {
    endpointProgram = new PublicKey(must("ENDPOINT_PROGRAM"));
    console.log("REAL LZ endpoint:", endpointProgram.toBase58());
  }

  const [oappPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("OApp")],
    program.programId
  );

  const srcEidBuf = Buffer.alloc(4);
  srcEidBuf.writeUInt32BE(srcEid >>> 0, 0);

  const [peerPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("Peer"), oappPda.toBuffer(), srcEidBuf],
    program.programId
  );

  console.log({
    programId: program.programId.toBase58(),
    oappPda: oappPda.toBase58(),
    peerPda: peerPda.toBase58(),
    srcEid,
    peerHex: Buffer.from(peerBytes).toString("hex"),
  });

  try {
    const tx1 = await program.methods
      .initOapp(endpointProgram, admin.publicKey)
      .accounts({
        admin: admin.publicKey,
        // oappConfig: oappPda,
        // systemProgram: SystemProgram.programId,
      })
      .rpc();
    console.log("init_oapp", tx1);
  } catch (e: any) {
    console.log("init_oapp skipped:", e.message ?? e);
  }

  const tx2 = await program.methods
    .setPeer(srcEid, peerBytes)
    .accountsPartial({
      admin: admin.publicKey,
      oappConfig: oappPda,
      peerConfig: peerPda,
    //   systemProgram: SystemProgram.programId,
    })
    .rpc();

  console.log("set_peer", tx2);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});