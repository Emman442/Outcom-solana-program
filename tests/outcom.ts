import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Outcom } from "../target/types/outcom";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { expect } from "chai";

describe("outcom", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Outcom as Program<Outcom>;

  const employer = Keypair.generate();
  const candidate = Keypair.generate();
  const referrer = Keypair.generate();
  const mockEndpoint = Keypair.generate();

  const trialId = "trial_1";
  const title = "Build a Solana Payment Integration";
  const description = "Integrate USDC transfers on devnet";
  const category = "Smart Contracts";
  const skills = "Solana,Anchor,Rust";
  const difficulty = "Advanced";
  const objective = "Ship a working USDC payment flow";
  const requirements = "Anchor program, tests, public repo";
  const candidateReward = new anchor.BN(450_000_000);
  const referralReward = new anchor.BN(50_000_000);
  const srcEid = 1;
  const mockPeer = Buffer.alloc(32, 1);

  let usdcMint: PublicKey;
  let employerAta: PublicKey;
  let candidateAta: PublicKey;
  let referrerAta: PublicKey;
  let oappPda: PublicKey;
  let peerPda: PublicKey;
  let trialPda: PublicKey;
  let vaultPda: PublicKey;
  let referralPda: PublicKey;

  function encodePayload(
    id: string,
    cand: PublicKey,
    ref: PublicKey | null,
    score: number,
    passed: boolean
  ) {
    const trialIdBuffer = Buffer.alloc(32);
    trialIdBuffer.write(id, "utf-8");
    const scoreBuffer = Buffer.alloc(8);
    scoreBuffer.writeBigUInt64BE(BigInt(score), 0);
    return Buffer.concat([
      trialIdBuffer,
      cand.toBuffer(),
      ref ? ref.toBuffer() : Buffer.alloc(32),
      scoreBuffer,
      Buffer.from([passed ? 1 : 0]),
    ]);
  }

  before(async () => {
    for (const kp of [employer, candidate, referrer, mockEndpoint]) {
      const sig = await provider.connection.requestAirdrop(
        kp.publicKey,
        2 * anchor.web3.LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    usdcMint = await createMint(
      provider.connection,
      employer,
      employer.publicKey,
      null,
      6
    );
    employerAta = await createAccount(
      provider.connection,
      employer,
      usdcMint,
      employer.publicKey
    );
    candidateAta = await createAccount(
      provider.connection,
      candidate,
      usdcMint,
      candidate.publicKey
    );
    referrerAta = await createAccount(
      provider.connection,
      referrer,
      usdcMint,
      referrer.publicKey
    );
    await mintTo(
      provider.connection,
      employer,
      usdcMint,
      employerAta,
      employer,
      1_000_000_000
    );

    [oappPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("OApp")],
      program.programId
    );
    const srcEidBuf = Buffer.alloc(4);
    srcEidBuf.writeUInt32BE(srcEid, 0);
    [peerPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("Peer"), oappPda.toBuffer(), srcEidBuf],
      program.programId
    );
    [trialPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("Trial"), employer.publicKey.toBuffer(), Buffer.from(trialId)],
      program.programId
    );
    [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("Vault"), trialPda.toBuffer()],
      program.programId
    );
    [referralPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("Referral"), trialPda.toBuffer(), candidate.publicKey.toBuffer()],
      program.programId
    );
  });

  it("initializes oapp and peer", async () => {
    await program.methods
      .initOapp(mockEndpoint.publicKey, employer.publicKey)
      .accountsPartial({ admin: employer.publicKey })
      .signers([employer])
      .rpc();

    const oapp = await program.account.oAppConfig.fetch(oappPda);
    expect(oapp.admin.toBase58()).to.equal(employer.publicKey.toBase58());
    expect(oapp.endpointProgram.toBase58()).to.equal(
      mockEndpoint.publicKey.toBase58()
    );

    await program.methods
      .setPeer(srcEid, Array.from(mockPeer))
      .accountsPartial({
        admin: employer.publicKey,
        oappConfig: oappPda,
        peerConfig: peerPda,
      })
      .signers([employer])
      .rpc();

    const peer = await program.account.peerConfig.fetch(peerPda);
    expect(peer.srcEid).to.equal(srcEid);
  });

  it("initializes a funded trial", async () => {
    await program.methods
      .initializeTrial(
        trialId,
        title,
        description,
        category,
        skills,
        difficulty,
        objective,
        requirements,
        candidateReward,
        referralReward
      )
      .accountsPartial({
        employer: employer.publicKey,
        usdcMint,
        employerTokenAccount: employerAta,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([employer])
      .rpc();

    const trial = await program.account.trialAccount.fetch(trialPda);
    expect(trial.trialId).to.equal(trialId);
    expect(trial.title).to.equal(title);
    expect(trial.requirements).to.equal(requirements);
    expect(trial.candidateReward.toString()).to.equal(candidateReward.toString());
    expect(trial.totalLocked.toString()).to.equal("500000000");
    expect(Object.keys(trial.status)[0]).to.equal("open");

    const vault = await getAccount(provider.connection, vaultPda);
    expect(vault.amount.toString()).to.equal("500000000");
  });

  it("rejects self-referral", async () => {
    try {
      await program.methods
        .referCandidate(referrer.publicKey, "cannot refer myself")
        .accountsPartial({
          referrer: referrer.publicKey,
          trialAccount: trialPda,
        })
        .signers([referrer])
        .rpc();
      expect.fail("should have thrown");
    } catch (e: any) {
      expect(String(e)).to.match(/CannotReferSelf|custom program error/i);
    }
  });

  it("records a referral", async () => {
    await program.methods
      .referCandidate(candidate.publicKey, "strong rust engineer")
      .accountsPartial({
        referrer: referrer.publicKey,
        trialAccount: trialPda,
      })
      .signers([referrer])
      .rpc();

    const rec = await program.account.referral.fetch(referralPda);
    expect(rec.trial.toBase58()).to.equal(trialPda.toBase58());
    expect(rec.trialId).to.equal(trialId);
    expect(rec.referrer.toBase58()).to.equal(referrer.publicKey.toBase58());
    expect(rec.candidate.toBase58()).to.equal(candidate.publicKey.toBase58());
    expect(rec.note).to.equal("strong rust engineer");
  });

  it("starts the trial", async () => {
    await program.methods
      .startTrial()
      .accountsPartial({
        candidate: candidate.publicKey,
        trialAccount: trialPda,
      })
      .signers([candidate])
      .rpc();

    const trial = await program.account.trialAccount.fetch(trialPda);
    expect(Object.keys(trial.status)[0]).to.equal("inProgress");
    expect(trial.selectedCandidate.toBase58()).to.equal(
      candidate.publicKey.toBase58()
    );
  });

  it("rejects a failing verdict without paying", async () => {
    const otherId = "trial_fail";
    const [failTrial] = PublicKey.findProgramAddressSync(
      [Buffer.from("Trial"), employer.publicKey.toBuffer(), Buffer.from(otherId)],
      program.programId
    );

    await program.methods
      .initializeTrial(
        otherId,
        title,
        description,
        category,
        skills,
        difficulty,
        objective,
        requirements,
        candidateReward,
        referralReward
      )
      .accountsPartial({
        employer: employer.publicKey,
        usdcMint,
        employerTokenAccount: employerAta,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([employer])
      .rpc();

    await program.methods
      .startTrial()
      .accountsPartial({
        candidate: candidate.publicKey,
        trialAccount: failTrial,
      })
      .signers([candidate])
      .rpc();

    const [failVault] = PublicKey.findProgramAddressSync(
      [Buffer.from("Vault"), failTrial.toBuffer()],
      program.programId
    );

    await program.methods
      .lzReceive({
        srcEid,
        sender: Array.from(mockPeer),
        nonce: new anchor.BN(1),
        guid: Array.from(Buffer.alloc(32)),
        payload: encodePayload(otherId, candidate.publicKey, null, 40, false),
        extraData: Buffer.from([]),
      })
      .accountsPartial({
        endpointProgram: mockEndpoint.publicKey,
        oappConfig: oappPda,
        peerConfig: peerPda,
        trialAccount: failTrial,
        vaultAccount: failVault,
        candidateTokenAccount: candidateAta,
        referrerTokenAccount: null,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([mockEndpoint])
      .rpc();

    const trial = await program.account.trialAccount.fetch(failTrial);
    expect(Object.keys(trial.status)[0]).to.equal("rejected");
    const cand = await getAccount(provider.connection, candidateAta);
    expect(cand.amount.toString()).to.equal("0");
  });

  it("pays candidate and referrer on passing lz_receive", async () => {
    await program.methods
      .lzReceive({
        srcEid,
        sender: Array.from(mockPeer),
        nonce: new anchor.BN(2),
        guid: Array.from(Buffer.alloc(32)),
        payload: encodePayload(
          trialId,
          candidate.publicKey,
          referrer.publicKey,
          85,
          true
        ),
        extraData: Buffer.from([]),
      })
      .accountsPartial({
        endpointProgram: mockEndpoint.publicKey,
        oappConfig: oappPda,
        peerConfig: peerPda,
        trialAccount: trialPda,
        vaultAccount: vaultPda,
        candidateTokenAccount: candidateAta,
        referrerTokenAccount: referrerAta,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([mockEndpoint])
      .rpc();

    const trial = await program.account.trialAccount.fetch(trialPda);
    expect(Object.keys(trial.status)[0]).to.equal("paid");

    const cand = await getAccount(provider.connection, candidateAta);
    const ref = await getAccount(provider.connection, referrerAta);
    const vault = await getAccount(provider.connection, vaultPda);
    expect(cand.amount.toString()).to.equal("450000000");
    expect(ref.amount.toString()).to.equal("50000000");
    expect(vault.amount.toString()).to.equal("0");
  });
});