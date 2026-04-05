const { ClobClient } = require('@polymarket/clob-client');
const { ethers } = require('ethers');

async function main() {
    const pk = "0x921c0ef3157b8d2ee10f19d76f6a47231221fa990feff54e4cdc64895de80822";
    const funderAddress = "0x8beB5585e5A5f4889294d09235cbA5966a1f3Bea";
    const wallet = new ethers.Wallet(pk);
    
    console.log("EOA Address:", wallet.address);
    console.log("Testing Funder Address (from Username):", funderAddress);

    // Using GNOSIS_SAFE signature type
    const client = new ClobClient("https://clob.polymarket.com", 137, wallet, undefined, 2, funderAddress);
    
    try {
        const creds = await client.createOrDeriveApiKey();
        console.log("SUCCESS! EOA is an authorized signer for Proxy:", funderAddress);
        console.log("Derived API_KEY:", creds.apiKey);
        console.log("Derived API_SECRET:", creds.secret);
        console.log("Derived API_PASSPHRASE:", creds.passphrase);
    } catch (e) {
        console.error("FAILURE! Could not derive API key. The proxy may not belong to this EOA.", e.message);
        process.exit(1);
    }
}

main();