import {
    address,
    appendTransactionMessageInstructions,
    assertIsTransactionWithinSizeLimit,
    createKeyPairSignerFromBytes,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    createTransactionMessage,
    devnet,
    getSignatureFromTransaction,
    pipe,
    sendAndConfirmTransactionFactory,
    setTransactionMessageFeePayerSigner,
    setTransactionMessageLifetimeUsingBlockhash,
    signTransactionMessageWithSigners,
    addSignersToTransactionMessage,
    getProgramDerivedAddress,
    generateKeyPairSigner,
    getAddressEncoder,
    getBytesEncoder,
} from "@solana/kit";
import { getInitializeInstruction, getSubmitTsInstruction } from "./clients/js/src/generated/index";

const MPL_CORE_PROGRAM = 
address("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d"); 
const PROGRAM_ADDRESS = 
address("TRBZyQHB3m68FGeVsqTK39Wm4xejadjVhP5MAZaKWDM"); 
const SYSTEM_PROGRAM = address("11111111111111111111111111111111"); 
const COLLECTION = address("5ebsp5RChCGK7ssRZMVMufgVZhd2kFbNaotcZ5UvytN2");

// Import your Turbin3 wallet
import wallet from "./wallet.json";

async function performEnroll() {
    // Import keypair from wallet file
    const keypair = await createKeyPairSignerFromBytes(new Uint8Array(wallet));
    console.log(`Your Solana wallet address: ${keypair.address}`);

    // Create RPC connection
    const rpc = createSolanaRpc(devnet("https://api.devnet.solana.com"));
    const rpcSubscriptions = createSolanaRpcSubscriptions(
        devnet("ws://api.devnet.solana.com"),
    );

    const addressEncoder = getAddressEncoder();
    const bytesEncoder = getBytesEncoder();

    // Create the PDA for enrollment account
    const accountSeeds = [
        Buffer.from("prereqs"),
        addressEncoder.encode(keypair.address)
    ];
    const [account, _bump] = await getProgramDerivedAddress({
        programAddress: PROGRAM_ADDRESS,
        seeds: accountSeeds
    });

    // Create the authority PDA (required for submitTs)
    const authoritySeeds = [
        bytesEncoder.encode(new Uint8Array([99, 111, 108, 108, 101, 99, 116, 105, 111, 110])), // "collection" in bytes
        addressEncoder.encode(COLLECTION)
    ];
    const [authority, _authorityBump] = await getProgramDerivedAddress({
        programAddress: PROGRAM_ADDRESS,
        seeds: authoritySeeds
    });

    // mint keypair for the NFT
    const mintKeyPair = await generateKeyPairSigner();

    const sendAndConfirmTransaction = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

    // Fetch latest blockhash
    const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();

    try {
        // the initialize transaction
        console.log("Executing initialize transaction...");
        const initializeIx = getInitializeInstruction({
            github: "Nishu0",
            user: keypair,
            account,
            systemProgram: SYSTEM_PROGRAM
        });

        const transactionMessageInit = pipe(
            createTransactionMessage({ version: 0 }),
            tx => setTransactionMessageFeePayerSigner(keypair, tx),
            tx => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx),
            tx => appendTransactionMessageInstructions([initializeIx], tx)
        );

        const signedTxInit = await signTransactionMessageWithSigners(transactionMessageInit);
        assertIsTransactionWithinSizeLimit(signedTxInit);

        const initResult = await sendAndConfirmTransaction(
            signedTxInit,
            { commitment: 'confirmed', skipPreflight: false }
        );
        console.log("Initialize transaction result:", initResult);
        
        const signatureInit = getSignatureFromTransaction(signedTxInit);
        console.log(`Initialize Success! Check out your TX here: https://explorer.solana.com/tx/${signatureInit}?cluster=devnet`);

        await new Promise(resolve => setTimeout(resolve, 2000));

        // submitTs transaction
        console.log("Executing submitTs transaction...");
        const submitIx = getSubmitTsInstruction({
            user: keypair,
            account,
            mint: mintKeyPair,
            collection: COLLECTION,
            authority,
            mplCoreProgram: MPL_CORE_PROGRAM,
            systemProgram: SYSTEM_PROGRAM
        });

        const { value: latestBlockhash2 } = await rpc.getLatestBlockhash().send();

        const transactionMessageSubmit = pipe(
            createTransactionMessage({ version: 0 }),
            tx => setTransactionMessageFeePayerSigner(keypair, tx),
            tx => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash2, tx),
            tx => appendTransactionMessageInstructions([submitIx], tx),
            tx => addSignersToTransactionMessage([mintKeyPair], tx) // Add mint as additional signer
        );

        const signedTxSubmit = await signTransactionMessageWithSigners(transactionMessageSubmit);
        assertIsTransactionWithinSizeLimit(signedTxSubmit);

        const submitResult = await sendAndConfirmTransaction(
            signedTxSubmit,
            { commitment: 'confirmed', skipPreflight: false }
        );
        console.log("SubmitTs transaction result:", submitResult);
        
        const signatureSubmit = getSignatureFromTransaction(signedTxSubmit);
        console.log(`SubmitTs Success! Check out your TX here: https://explorer.solana.com/tx/${signatureSubmit}?cluster=devnet`);

    } catch (e) {
        console.error(`Oops, something went wrong: ${e}`);
    }
}

// Execute the async function
performEnroll().catch(console.error);