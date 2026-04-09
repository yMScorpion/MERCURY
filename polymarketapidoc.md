> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Authentication

> How to authenticate requests to the CLOB API

The CLOB API uses two levels of authentication: **L1 (Private Key)** and **L2 (API Key)**. Either can be accomplished using the CLOB client or REST API.

## Public vs Authenticated

<CardGroup cols={1}>
  <Card title="Public (No Auth)" icon="unlock">
    The **Gamma API**, **Data API**, and CLOB read endpoints (orderbook, prices, spreads) require no authentication.
  </Card>

  <Card title="Authenticated (CLOB)" icon="lock">
    CLOB trading endpoints (placing orders, cancellations, heartbeat) require all 5 `POLY_*` L2 HTTP headers.
  </Card>
</CardGroup>

***

## Two-Level Authentication Model

The CLOB uses two levels of authentication: L1 (Private Key) and L2 (API Key). Either can be accomplished using the CLOB client or REST API

### L1 Authentication

L1 authentication uses the wallet's private key to sign an EIP-712 message used in the request header. It proves ownership and control over the private key. The private key stays in control of the user and all trading activity remains non-custodial.

**Used for:**

* Creating API credentials
* Deriving existing API credentials
* Signing and creating user's orders locally

### L2 Authentication

L2 uses API credentials (apiKey, secret, passphrase) generated from L1 authentication. These are used solely to authenticate requests made to the CLOB API. Requests are signed using HMAC-SHA256.

**Used for:**

* Cancel or get user's open orders
* Check user's balances and allowances
* Post user's signed orders

<Info>
  Even with L2 authentication headers, methods that create user orders still
  require the user to sign the order payload.
</Info>

***

## Getting API Credentials

Before making authenticated requests, you need to obtain API credentials using L1 authentication.

### Using the SDK

<Tabs>
  <Tab title="TypeScript">
    ```typescript  theme={null}
    import { ClobClient } from "@polymarket/clob-client";
    import { Wallet } from "ethers"; // v5.8.0

    const client = new ClobClient(
      "https://clob.polymarket.com",
      137, // Polygon mainnet
      new Wallet(process.env.PRIVATE_KEY)
    );

    // Creates new credentials or derives existing ones
    const credentials = await client.createOrDeriveApiKey();

    console.log(credentials);
    // {
    //   apiKey: "550e8400-e29b-41d4-a716-446655440000",
    //   secret: "base64EncodedSecretString",
    //   passphrase: "randomPassphraseString"
    // }
    ```
  </Tab>

  <Tab title="Python">
    ```python  theme={null}
    from py_clob_client.client import ClobClient
    import os

    client = ClobClient(
        host="https://clob.polymarket.com",
        chain_id=137,  # Polygon mainnet
        key=os.getenv("PRIVATE_KEY")
    )

    # Creates new credentials or derives existing ones
    credentials = client.create_or_derive_api_creds()

    print(credentials)
    # {
    #     "apiKey": "550e8400-e29b-41d4-a716-446655440000",
    #     "secret": "base64EncodedSecretString",
    #     "passphrase": "randomPassphraseString"
    # }
    ```
  </Tab>

  <Tab title="Rust">
    ```rust  theme={null}
    use std::str::FromStr;
    use polymarket_client_sdk::POLYGON;
    use polymarket_client_sdk::auth::{LocalSigner, Signer};
    use polymarket_client_sdk::clob::{Client, Config};

    let private_key = std::env::var("POLYMARKET_PRIVATE_KEY")?;
    let signer = LocalSigner::from_str(&private_key)?
        .with_chain_id(Some(POLYGON));

    // Creates new credentials or derives existing ones,
    // then initializes the authenticated client — all in one step
    let client = Client::new("https://clob.polymarket.com", Config::default())?
        .authentication_builder(&signer)
        .authenticate()
        .await?;

    let credentials = client.credentials();
    println!("API Key: {}", credentials.key());
    ```
  </Tab>
</Tabs>

<Warning>
  **Never commit private keys to version control.** Always use environment
  variables or secure key management systems.
</Warning>

### Using the REST API

While we highly recommend using our provided clients to handle signing and authentication, the following is for developers who choose NOT to use our [Python](https://github.com/Polymarket/py-clob-client) or [TypeScript](https://github.com/Polymarket/clob-client) clients.

**Create API Credentials**

```bash  theme={null}
POST https://clob.polymarket.com/auth/api-key
```

**Derive API Credentials**

```bash  theme={null}
GET https://clob.polymarket.com/auth/derive-api-key
```

Required L1 headers:

| Header           | Description            |
| ---------------- | ---------------------- |
| `POLY_ADDRESS`   | Polygon signer address |
| `POLY_SIGNATURE` | CLOB EIP-712 signature |
| `POLY_TIMESTAMP` | Current UNIX timestamp |
| `POLY_NONCE`     | Nonce (default: 0)     |

The `POLY_SIGNATURE` is generated by signing the following EIP-712 struct:

<Accordion title="EIP-712 Signing Example">
  <CodeGroup>
    ```typescript TypeScript theme={null}
    const domain = {
      name: "ClobAuthDomain",
      version: "1",
      chainId: chainId, // Polygon Chain ID 137
    };

    const types = {
      ClobAuth: [
        { name: "address", type: "address" },
        { name: "timestamp", type: "string" },
        { name: "nonce", type: "uint256" },
        { name: "message", type: "string" },
      ],
    };

    const value = {
      address: signingAddress, // The Signing address
      timestamp: ts,            // The CLOB API server timestamp
      nonce: nonce,             // The nonce used
      message: "This message attests that I control the given wallet",
    };

    const sig = await signer._signTypedData(domain, types, value);
    ```

    ```python Python theme={null}
    domain = {
        "name": "ClobAuthDomain",
        "version": "1",
        "chainId": chainId,  # Polygon Chain ID 137
    }

    types = {
        "ClobAuth": [
            {"name": "address", "type": "address"},
            {"name": "timestamp", "type": "string"},
            {"name": "nonce", "type": "uint256"},
            {"name": "message", "type": "string"},
        ]
    }

    value = {
        "address": signingAddress,  # The signing address
        "timestamp": ts,            # The CLOB API server timestamp
        "nonce": nonce,             # The nonce used
        "message": "This message attests that I control the given wallet",
    }

    sig = signer.sign_typed_data(domain, types, value)
    ```
  </CodeGroup>
</Accordion>

Reference implementations:

* [TypeScript](https://github.com/Polymarket/clob-client/blob/main/src/signing/eip712.ts)
* [Python](https://github.com/Polymarket/py-clob-client/blob/main/py_clob_client/signing/eip712.py)

Response:

```json  theme={null}
{
  "apiKey": "550e8400-e29b-41d4-a716-446655440000",
  "secret": "base64EncodedSecretString",
  "passphrase": "randomPassphraseString"
}
```

**You'll need all three values for L2 authentication.**

***

## L2 Authentication Headers

All trading endpoints require these 5 headers:

| Header            | Description                   |
| ----------------- | ----------------------------- |
| `POLY_ADDRESS`    | Polygon signer address        |
| `POLY_SIGNATURE`  | HMAC signature for request    |
| `POLY_TIMESTAMP`  | Current UNIX timestamp        |
| `POLY_API_KEY`    | User's API `apiKey` value     |
| `POLY_PASSPHRASE` | User's API `passphrase` value |

The `POLY_SIGNATURE` for L2 is an HMAC-SHA256 signature created using the user's API credentials `secret` value. Reference implementations can be found in the [TypeScript](https://github.com/Polymarket/clob-client/blob/main/src/signing/hmac.ts) and [Python](https://github.com/Polymarket/py-clob-client/blob/main/py_clob_client/signing/hmac.py) clients.

### CLOB Client

<Tabs>
  <Tab title="TypeScript">
    ```typescript  theme={null}
    import { ClobClient } from "@polymarket/clob-client";
    import { Wallet } from "ethers"; // v5.8.0

    const client = new ClobClient(
      "https://clob.polymarket.com",
      137,
      new Wallet(process.env.PRIVATE_KEY),
      apiCreds, // Generated from L1 auth, API credentials enable L2 methods
      1, // signatureType explained below
      funderAddress // funder explained below
    );

    // Now you can trade!
    const order = await client.createAndPostOrder(
      { tokenID: "123456", price: 0.65, size: 100, side: "BUY" },
      { tickSize: "0.01", negRisk: false }
    );
    ```
  </Tab>

  <Tab title="Python">
    ```python  theme={null}
    from py_clob_client.client import ClobClient
    import os

    client = ClobClient(
        host="https://clob.polymarket.com",
        chain_id=137,
        key=os.getenv("PRIVATE_KEY"),
        creds=api_creds,  # Generated from L1 auth, API credentials enable L2 methods
        signature_type=1,  # signatureType explained below
        funder=os.getenv("FUNDER_ADDRESS") # funder explained below
    )

    # Now you can trade!
    order = client.create_and_post_order(
        {"token_id": "123456", "price": 0.65, "size": 100, "side": "BUY"},
        {"tick_size": "0.01", "neg_risk": False}
    )
    ```
  </Tab>

  <Tab title="Rust">
    ```rust  theme={null}
    use polymarket_client_sdk::clob::types::{Side, SignatureType};
    use polymarket_client_sdk::types::dec;

    let client = Client::new("https://clob.polymarket.com", Config::default())?
        .authentication_builder(&signer)
        .signature_type(SignatureType::Proxy) // signatureType explained below
        // Funder auto-derived via CREATE2 for Proxy/GnosisSafe
        .authenticate()
        .await?;

    // Now you can trade!
    let order = client.limit_order()
        .token_id("123456".parse()?)
        .price(dec!(0.65))
        .size(dec!(100))
        .side(Side::Buy)
        .build().await?;
    let signed = client.sign(&signer, order).await?;
    let response = client.post_order(signed).await?;
    ```
  </Tab>
</Tabs>

<Info>
  Even with L2 authentication headers, methods that create user orders still
  require the user to sign the order payload.
</Info>

***

## Signature Types and Funder

When initializing the L2 client, you must specify your wallet **signatureType** and the **funder** address which holds the funds:

| Signature Type | Value | Description                                                                                                                                                                                  |
| -------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| EOA            | `0`   | Standard Ethereum wallet (MetaMask). Funder is the EOA address and will need POL to pay gas on transactions.                                                                                 |
| POLY\_PROXY    | `1`   | A custom proxy wallet only used with users who logged in via Magic Link email/Google. Using this requires the user to have exported their PK from Polymarket.com and imported into your app. |
| GNOSIS\_SAFE   | `2`   | Gnosis Safe multisig proxy wallet (most common). Use this for any new or returning user who does not fit the other 2 types.                                                                  |

<Tip>
  The wallet address displayed to the user on Polymarket.com is the proxy wallet
  and should be used as the funder. These can be deterministically derived or
  you can deploy them on behalf of the user. These proxy wallets are
  automatically deployed for the user on their first login to Polymarket.com.
</Tip>

***

## Security Best Practices

<AccordionGroup>
  <Accordion title="Never expose private keys">
    Store private keys in environment variables or secure key management systems. Never commit them to version control.

    ```bash  theme={null}
    # .env (never commit this file)
    PRIVATE_KEY=0x...
    ```
  </Accordion>

  <Accordion title="Implement request signing on the server">
    Never expose your API secret in client-side code. All authenticated requests should originate from your backend.
  </Accordion>
</AccordionGroup>

***

## Troubleshooting

<AccordionGroup>
  <Accordion title="Error - INVALID_SIGNATURE">
    Your wallet's private key is incorrect or improperly formatted.

    **Solutions:**

    * Verify your private key is a valid hex string (starts with "0x")
    * Ensure you're using the correct key for the intended address
    * Check that the key has proper permissions
  </Accordion>

  <Accordion title="Error - NONCE_ALREADY_USED">
    The nonce you provided has already been used to create an API key.

    **Solutions:**

    * Use `deriveApiKey()` with the same nonce to retrieve existing credentials
    * Or use a different nonce with `createApiKey()`
  </Accordion>

  <Accordion title="Error - Invalid Funder Address">
    Your funder address is incorrect or doesn't match your wallet.

    **Solution:** Check your Polymarket profile address at [polymarket.com/settings](https://polymarket.com/settings).

    If it does not exist or user has never logged into Polymarket.com, deploy it first before creating L2 authentication.
  </Accordion>

  <Accordion title="Lost both credentials and nonce">
    Unfortunately, there's no way to recover lost API credentials without the nonce. You'll need to create new credentials:

    ```typescript  theme={null}
    // Create fresh credentials with a new nonce
    const newCreds = await client.createApiKey();
    // Save the nonce this time!
    ```
  </Accordion>
</AccordionGroup>

***

## Next Steps

<CardGroup cols={2}>
  <Card title="Place Your First Order" icon="plus" href="/trading/quickstart">
    Learn how to create and submit orders.
  </Card>

  <Card title="Geographic Restrictions" icon="globe" href="/api-reference/geoblock">
    Check trading availability by region.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Clients & SDKs

> Official open-source libraries for interacting with Polymarket

Polymarket provides official open-source clients in TypeScript, Python, and Rust. All three support the full CLOB API including market data, order management, and authentication.

## Installation

<CodeGroup>
  ```bash TypeScript theme={null}
  npm install @polymarket/clob-client ethers@5
  ```

  ```bash Python theme={null}
  pip install py-clob-client
  ```

  ```bash Rust theme={null}
  cargo add polymarket-client-sdk
  ```
</CodeGroup>

## Quick Example

<CodeGroup>
  ```typescript TypeScript theme={null}
  import { ClobClient } from "@polymarket/clob-client";

  const client = new ClobClient(
    "https://clob.polymarket.com",
    137,
    signer,
    apiCreds,
  );

  const markets = await client.getMarkets();
  ```

  ```python Python theme={null}
  from py_clob_client.client import ClobClient

  client = ClobClient(
      "https://clob.polymarket.com",
      key=private_key,
      chain_id=137,
      creds=api_creds,
  )

  markets = client.get_markets()
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::{Client, Config};

  let client = Client::new("https://clob.polymarket.com", Config::default())?
      .authentication_builder(&signer)
      .authenticate()
      .await?;

  let markets = client.markets(None).await?;
  ```
</CodeGroup>

## Source Code

| Language   | Package                   | Repository                                                                           |
| ---------- | ------------------------- | ------------------------------------------------------------------------------------ |
| TypeScript | `@polymarket/clob-client` | [github.com/Polymarket/clob-client](https://github.com/Polymarket/clob-client)       |
| Python     | `py-clob-client`          | [github.com/Polymarket/py-clob-client](https://github.com/Polymarket/py-clob-client) |
| Rust       | `polymarket-client-sdk`   | [github.com/Polymarket/rs-clob-client](https://github.com/Polymarket/rs-clob-client) |

Each repository includes working examples in the `/examples` directory.

## Builder SDKs

If you're building an app through the [Builder Program](/builders/overview), additional signing SDKs are available:

| Language   | Package                           | Repository                                                                                           |
| ---------- | --------------------------------- | ---------------------------------------------------------------------------------------------------- |
| TypeScript | `@polymarket/builder-signing-sdk` | [github.com/Polymarket/builder-signing-sdk](https://github.com/Polymarket/builder-signing-sdk)       |
| Python     | `py_builder_signing_sdk`          | [github.com/Polymarket/py-builder-signing-sdk](https://github.com/Polymarket/py-builder-signing-sdk) |

See [Order Attribution](/trading/orders/attribution) for usage details.

## Relayer SDK

For [gasless transactions](/trading/gasless) using proxy wallets, the relayer client handles submitting transactions through Polymarket's relayer:

| Language   | Package                              | Repository                                                                                                 |
| ---------- | ------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| TypeScript | `@polymarket/builder-relayer-client` | [github.com/Polymarket/builder-relayer-client](https://github.com/Polymarket/builder-relayer-client)       |
| Python     | `py-builder-relayer-client`          | [github.com/Polymarket/py-builder-relayer-client](https://github.com/Polymarket/py-builder-relayer-client) |

## Next Steps

<CardGroup cols={2}>
  <Card title="Quickstart" icon="rocket" href="/quickstart">
    Set up your client and place your first order.
  </Card>

  <Card title="Authentication" icon="lock" href="/api-reference/authentication">
    Understand L1/L2 auth and API credentials.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Rate Limits

> API rate limits for all Polymarket endpoints

All API rate limits are enforced using Cloudflare's throttling system. When you exceed the limit for any endpoint, requests are throttled (delayed/queued) rather than immediately rejected. Limits reset on sliding time windows.

***

## General

| Endpoint              | Limit            |
| --------------------- | ---------------- |
| General rate limiting | 15,000 req / 10s |
| Health check (`/ok`)  | 100 req / 10s    |

***

## Gamma API

Base URL: `https://gamma-api.polymarket.com`

| Endpoint                       | Limit           |
| ------------------------------ | --------------- |
| General                        | 4,000 req / 10s |
| `/events`                      | 500 req / 10s   |
| `/markets`                     | 300 req / 10s   |
| `/markets` + `/events` listing | 900 req / 10s   |
| `/comments`                    | 200 req / 10s   |
| `/tags`                        | 200 req / 10s   |
| `/public-search`               | 350 req / 10s   |

***

## Data API

Base URL: `https://data-api.polymarket.com`

| Endpoint             | Limit           |
| -------------------- | --------------- |
| General              | 1,000 req / 10s |
| `/trades`            | 200 req / 10s   |
| `/positions`         | 150 req / 10s   |
| `/closed-positions`  | 150 req / 10s   |
| Health check (`/ok`) | 100 req / 10s   |

***

## CLOB API

Base URL: `https://clob.polymarket.com`

### General

| Endpoint                   | Limit           |
| -------------------------- | --------------- |
| General                    | 9,000 req / 10s |
| `GET` balance allowance    | 200 req / 10s   |
| `UPDATE` balance allowance | 50 req / 10s    |

### Market Data

| Endpoint          | Limit           |
| ----------------- | --------------- |
| `/book`           | 1,500 req / 10s |
| `/books`          | 500 req / 10s   |
| `/price`          | 1,500 req / 10s |
| `/prices`         | 500 req / 10s   |
| `/midpoint`       | 1,500 req / 10s |
| `/midpoints`      | 500 req / 10s   |
| `/prices-history` | 1,000 req / 10s |
| Market tick size  | 200 req / 10s   |

### Ledger

| Endpoint                                         | Limit         |
| ------------------------------------------------ | ------------- |
| `/trades`, `/orders`, `/notifications`, `/order` | 900 req / 10s |
| `/data/orders`                                   | 500 req / 10s |
| `/data/trades`                                   | 500 req / 10s |
| `/notifications`                                 | 125 req / 10s |

### Authentication

| Endpoint          | Limit         |
| ----------------- | ------------- |
| API key endpoints | 100 req / 10s |

### Trading

Trading endpoints have both **burst** limits (short spikes allowed) and **sustained** limits (longer-term average).

| Endpoint                       | Burst Limit     | Sustained Limit     |
| ------------------------------ | --------------- | ------------------- |
| `POST /order`                  | 3,500 req / 10s | 36,000 req / 10 min |
| `DELETE /order`                | 3,000 req / 10s | 30,000 req / 10 min |
| `POST /orders`                 | 1,000 req / 10s | 15,000 req / 10 min |
| `DELETE /orders`               | 1,000 req / 10s | 15,000 req / 10 min |
| `DELETE /cancel-all`           | 250 req / 10s   | 6,000 req / 10 min  |
| `DELETE /cancel-market-orders` | 1,000 req / 10s | 1,500 req / 10 min  |

***

## Other

| Endpoint          | Limit          |
| ----------------- | -------------- |
| Relayer `/submit` | 25 req / 1 min |
| User PNL API      | 200 req / 10s  |

***

## Next Steps

<CardGroup cols={2}>
  <Card title="Authentication" icon="key" href="/api-reference/authentication">
    Learn how to authenticate trading requests.
  </Card>

  <Card title="Clients & SDKs" icon="cube" href="/api-reference/clients-sdks">
    Official TypeScript, Python, and Rust libraries.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Introduction

> Overview of the Polymarket APIs

The Polymarket API provides programmatic access to the world's largest prediction market. The platform is served by three separate APIs, each handling a different domain.

***

## APIs

<CardGroup cols={1}>
  <Card title="Gamma API" icon="database">
    **`https://gamma-api.polymarket.com`**

    Markets, events, tags, series, comments, sports, search, and public profiles. This is the primary API for discovering and browsing market data.
  </Card>

  <Card title="Data API" icon="chart-line">
    **`https://data-api.polymarket.com`**

    User positions, trades, activity, holder data, open interest, leaderboards, and builder analytics.
  </Card>

  <Card title="CLOB API" icon="arrows-rotate">
    **`https://clob.polymarket.com`**

    Orderbook data, pricing, midpoints, spreads, and price history. Also handles order placement, cancellation, and other trading operations. Trading endpoints require [authentication](/api-reference/authentication).
  </Card>
</CardGroup>

<Info>
  A separate **Bridge API** (`https://bridge.polymarket.com`) handles deposits and withdrawals. Bridges are not handled by Polymarket, it is a proxy of fun.xyz service.
</Info>

***

## Authentication

The Gamma API and Data API are fully public — no authentication required.

The CLOB API has both public endpoints (orderbook, prices) and authenticated endpoints (order management). See [Authentication](/api-reference/authentication) for details.

***

## Next Steps

<CardGroup cols={2}>
  <Card title="Authentication" icon="key" href="/api-reference/authentication">
    Learn how to authenticate requests for trading endpoints.
  </Card>

  <Card title="Clients & SDKs" icon="cube" href="/api-reference/clients-sdks">
    Official TypeScript, Python, and Rust libraries.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Geographic Restrictions

> Check geographic restrictions before placing orders on the Polymarket API

Polymarket restricts order placement from certain geographic locations due to regulatory requirements and compliance with international sanctions. Before placing orders, builders should verify the location.

<Warning>
  Orders submitted from blocked regions will be rejected. Implement geoblock
  checks in your application to provide users with appropriate feedback before
  they attempt to trade.
</Warning>

***

## Geoblock Endpoint

Check the geographic eligibility of the requesting IP address:

```bash  theme={null}
GET https://polymarket.com/api/geoblock
```

<Note>This endpoint is on `polymarket.com`, not the API servers.</Note>

### Response

```json  theme={null}
{
  "blocked": true,
  "ip": "203.0.113.42",
  "country": "US",
  "region": "NY"
}
```

| Field     | Type    | Description                                     |
| --------- | ------- | ----------------------------------------------- |
| `blocked` | boolean | Whether the user is blocked from placing orders |
| `ip`      | string  | Detected IP address                             |
| `country` | string  | ISO 3166-1 alpha-2 country code                 |
| `region`  | string  | Region/state code                               |

***

## Blocked Countries

The following countries are restricted from placing orders on Polymarket. Countries marked as **close-only** can close existing positions but cannot open new ones:

| Country Code | Country Name                         | Status     |
| ------------ | ------------------------------------ | ---------- |
| AU           | Australia                            | Blocked    |
| BE           | Belgium                              | Blocked    |
| BY           | Belarus                              | Blocked    |
| BI           | Burundi                              | Blocked    |
| CF           | Central African Republic             | Blocked    |
| CD           | Congo (Kinshasa)                     | Blocked    |
| CU           | Cuba                                 | Blocked    |
| DE           | Germany                              | Blocked    |
| ET           | Ethiopia                             | Blocked    |
| FR           | France                               | Blocked    |
| GB           | United Kingdom                       | Blocked    |
| IR           | Iran                                 | Blocked    |
| IQ           | Iraq                                 | Blocked    |
| IT           | Italy                                | Blocked    |
| KP           | North Korea                          | Blocked    |
| LB           | Lebanon                              | Blocked    |
| LY           | Libya                                | Blocked    |
| MM           | Myanmar                              | Blocked    |
| NI           | Nicaragua                            | Blocked    |
| NL           | Netherlands                          | Blocked    |
| PL           | Poland                               | Close-only |
| RU           | Russia                               | Blocked    |
| SG           | Singapore                            | Close-only |
| SO           | Somalia                              | Blocked    |
| SS           | South Sudan                          | Blocked    |
| SD           | Sudan                                | Blocked    |
| SY           | Syria                                | Blocked    |
| TH           | Thailand                             | Close-only |
| TW           | Taiwan                               | Close-only |
| UM           | United States Minor Outlying Islands | Blocked    |
| US           | United States                        | Blocked    |
| VE           | Venezuela                            | Blocked    |
| YE           | Yemen                                | Blocked    |
| ZW           | Zimbabwe                             | Blocked    |

***

## Blocked Regions

In addition to fully blocked countries, the following specific regions within otherwise accessible countries are also restricted:

| Country      | Region  | Region Code |
| ------------ | ------- | ----------- |
| Canada (CA)  | Ontario | ON          |
| Ukraine (UA) | Crimea  | 43          |
| Ukraine (UA) | Donetsk | 14          |
| Ukraine (UA) | Luhansk | 09          |

***

## Blocking Logic

The geoblocking system includes:

1. **OFAC-Sanctioned Countries**: Countries sanctioned by the U.S. Office of Foreign Assets Control (OFAC)
2. **Additional Regulatory Restrictions**: Countries added for specific regulatory compliance reasons

***

## Server Infrastructure

* **Primary Servers**: eu-west-2
* **Closest Non-Georestricted Region**: eu-west-1

***

## Usage Examples

<Tabs>
  <Tab title="TypeScript">
    ```typescript  theme={null}
    interface GeoblockResponse {
      blocked: boolean;
      ip: string;
      country: string;
      region: string;
    }

    async function checkGeoblock(): Promise<GeoblockResponse> {
      const response = await fetch("https://polymarket.com/api/geoblock");
      return response.json();
    }

    // Usage
    const geo = await checkGeoblock();

    if (geo.blocked) {
      console.log(`Trading not available in ${geo.country}`);
    } else {
      console.log("Trading available");
    }
    ```
  </Tab>

  <Tab title="Python">
    ```python  theme={null}
    import requests

    def check_geoblock() -> dict:
        response = requests.get("https://polymarket.com/api/geoblock")
        return response.json()

    # Usage
    geo = check_geoblock()

    if geo["blocked"]:
        print(f"Trading not available in {geo['country']}")
    else:
        print("Trading available")
    ```
  </Tab>

  <Tab title="Rust">
    ```rust  theme={null}
    use polymarket_client_sdk::clob::Client;

    let client = Client::default();
    let geo = client.check_geoblock().await?;

    if geo.blocked {
        println!("Trading not available in {}", geo.country);
    } else {
        println!("Trading available");
    }
    ```
  </Tab>
</Tabs>

***

## Why These Restrictions

Geographic restrictions are implemented to ensure compliance with:

* International sanctions and embargoes
* Local financial regulations
* Gambling and prediction market laws
* Anti-money laundering (AML) requirements
* Know Your Customer (KYC) regulations

If you believe you are incorrectly restricted or have questions about geographic availability, please contact [Polymarket Support](https://polymarket.com/support).

***

## Next Steps

<CardGroup cols={2}>
  <Card title="Authentication" icon="key" href="/api-reference/authentication">
    Learn how to authenticate trading requests.
  </Card>

  <Card title="Place Orders" icon="plus" href="/trading/quickstart">
    Start placing orders (from eligible regions).
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Polymarket 101

> An intro to Polymarket - the world's largest prediction market

Polymarket is a prediction market platform where users trade on the outcomes of real-world events. Instead of betting against a house, you trade shares with other users in an open, peer-to-peer market. Prices reflect the market's collective belief in the probability of an event occurring.

The platform is non-custodial, meaning you always control your funds. All trades are settled through smart contracts on the blockchain, ensuring transparent and trustless operation.

## Self-Custody

Polymarket operates on a non-custodial model. You maintain full control of your funds at all times.

* **You control your funds** - Assets are held in your wallet, secured by your private key
* **Smart contract enforcement** - Trades execute automatically through audited smart contracts
* **No intermediary risk** - Polymarket never takes possession of your funds — you maintain full control through your private key
* **Full transparency** - All trades and positions are recorded onchain and publicly verifiable
* **Trustless execution** - Settlement happens automatically based on market resolution

<Warning>
  Keep your private key safe and never share it with anyone. If you lose your
  private key, you lose access to your funds. If you signed up via Magic Link
  or have a proxy wallet, recovery may be possible through
  [recovery.polymarket.com](https://recovery.polymarket.com).
</Warning>

## How Polymarket Works

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/polymarket-101.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=059e9831d1c51b99996d9747c0139d49" alt="Polymarket Overview" className="dark:hidden" width="1526" height="952" data-path="images/core-concepts/polymarket-101.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/polymarket-101.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=4e929eca98a2bb83ef7421f7bbaf9f1d" alt="Polymarket Overview" className="hidden dark:block" width="1526" height="952" data-path="images/dark/core-concepts/polymarket-101.png" />
</Frame>

### Prices Are Probabilities

Every share on Polymarket is priced between `$0.00` and `$1.00`. The price represents the market's belief in the probability of that outcome occurring.

For example, if "Yes" shares for an event are trading at `$0.65`, the market believes there's approximately a `65%` chance the event will happen.

### Collateral and Tokens

Polymarket uses USDC.e (Bridged USDC on Polygon) as collateral. Every Yes/No pair is fully backed:

* `$1 USDC.e` creates one Yes share and one No share
* Winning shares are redeemable for `$1.00`
* Losing shares are worth `$0.00`

Shares are represented as tokens using the [Gnosis Conditional Token Framework](https://github.com/gnosis/conditional-tokens-contracts/) (ERC1155 standard), enabling seamless onchain trading and settlement.

### Trading

Polymarket uses a peer-to-peer order book (CLOB) for trading. You trade directly with other users, not against the house.

* **Buy shares** when you think the market underestimates the probability
* **Sell shares** when you think the market overestimates the probability
* **Exit anytime** - Sell your position before resolution to lock in profits or cut losses

| Action  | When to Use                           | Profit Scenario           |
| ------- | ------------------------------------- | ------------------------- |
| Buy Yes | You think the probability is too low  | Event occurs              |
| Buy No  | You think the probability is too high | Event does not occur      |
| Sell    | Lock in gains or limit losses         | Price moves in your favor |

### Resolution

When an event concludes, markets are resolved through the **UMA Optimistic Oracle**:

1. A proposer submits the outcome with a bond
2. There's a challenge period where anyone can dispute
3. If disputed, UMA token holders vote on the correct resolution
4. Winning tokens become redeemable for \$1 USDC.e

This community-driven process ensures fair and accurate market resolution.

## Why Blockchain

Polymarket is built on **Polygon**, a blockchain network, for several key reasons:

* **Global accessibility** - Anyone with an internet connection can participate
* **Non-custodial** - You control your funds, not a centralized entity
* **Transparent** - All activity is publicly verifiable onchain
* **Fast and affordable** - Polygon enables quick, low-cost transactions
* **Stable value** - USDC.e is pegged 1:1 to the US dollar, avoiding crypto volatility

## Proxy Wallets

When a user first uses Polymarket.com to trade they are prompted to create a wallet. When they do this, a 1 of 1 multisig is deployed to Polygon which is controlled/owned by the accessing EOA (either MetaMask wallet or MagicLink wallet). This proxy wallet is where all the user's positions (ERC1155) and USDC.e (ERC20) are held.

Using proxy wallets allows Polymarket to provide an improved UX where multi-step transactions can be executed atomically and transactions can be relayed by relayers on the gas station network. If you are a developer looking to programmatically access positions you accumulated via the Polymarket.com interface, you can either continue using the smart contract wallet by executing transactions through it from the owner account, or you can transfer these assets to a new address using the owner account.

### Deployments

Each user has their own proxy wallet (and thus proxy wallet address). See [Contract Addresses](/resources/contract-addresses) for all deployed factory and trading contract addresses on Polygon.

<Tip>
  For details on signature types (`EOA`, `POLY_PROXY`, `GNOSIS_SAFE`) and how to
  configure your trading client for each wallet type, see [Signature
  Types](/trading/overview#signature-types).
</Tip>

***

## Getting Started

Ready to start trading?

<CardGroup cols={2}>
  <Card title="Quickstart Guide" icon="rocket" href="/quickstart">
    Set up your account and make your first trade.
  </Card>

  <Card title="Explore Markets" icon="chart-line" href="https://polymarket.com">
    Browse active prediction markets on Polymarket.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Prices & Orderbook

> How prices work and how the order book enables peer-to-peer trading

Polymarket uses a **Central Limit Order Book (CLOB)** for trading. Prices aren't set by Polymarket—they emerge from supply and demand as users trade with each other.

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/orderbook.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=119174bcaaeb3b9abbd4c2d94b7bdae6" alt="" className="dark:hidden" width="1540" height="952" data-path="images/core-concepts/orderbook.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/orderbook.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=b940f4b5f28ab6ed5845dda2bfe03edb" alt="" className="hidden dark:block" width="1540" height="952" data-path="images/dark/core-concepts/orderbook.png" />
</Frame>

## Prices Are Probabilities

Every share on Polymarket is priced between `$0.00` and `$1.00`. The price directly represents the market's belief in the probability of that outcome.

| Price  | Implied Probability |
| ------ | ------------------- |
| \$0.25 | 25% chance          |
| \$0.50 | 50% chance          |
| \$0.75 | 75% chance          |

<Note>
  The displayed price is the **midpoint** of the bid-ask spread. If the spread
  is wider than \$0.10, the last traded price is shown instead.
</Note>

### Example

If the best bid for "Yes" is `$0.34` and the best ask is `$0.40`:

```
Displayed price = ($0.34 + $0.40) / 2 = $0.37 (37% probability)
```

You won't necessarily trade at `$0.37`—you'll pay the ask (`$0.40`) when buying or receive the bid (`$0.34`) when selling.

## The Order Book

The order book is a list of all open buy and sell orders for a market. It has two sides:

| Side | Description                                                 |
| ---- | ----------------------------------------------------------- |
| Bids | Buy orders—the highest prices traders are willing to pay    |
| Asks | Sell orders—the lowest prices traders are willing to accept |

The **spread** is the gap between the highest bid and lowest ask. Tighter spreads mean more liquid markets.

## Order Types

### Market Orders

Execute immediately at the best available price. Use when you want instant execution and are willing to pay the spread.

* **Buying**: You pay the lowest ask price
* **Selling**: You receive the highest bid price

### Limit Orders

Execute only at your specified price or better. Use when you want price control and are willing to wait.

* Your order sits in the book until someone trades against it
* Orders can **partially fill** as different traders match portions of your order
* You can cancel unfilled orders at any time

<Note>
  All orders on Polymarket are technically limit orders. A "market order" is
  simply a limit order priced to execute immediately against resting orders.
</Note>

## How Trades Work

Polymarket's CLOB is **hybrid-decentralized**:

1. **Offchain matching** — An operator matches compatible orders
2. **Onchain settlement** — Matched trades settle via smart contracts

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/trade-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=2acec8befdfbba57fb554170f7d5813c" alt="" className="dark:hidden" width="1540" height="952" data-path="images/core-concepts/trade-lifecycle.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/trade-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=d18b22ad7629820ad554dda8cb83ec18" alt="" className="hidden dark:block" width="1540" height="952" data-path="images/dark/core-concepts/trade-lifecycle.png" />
</Frame>

This design gives you the speed of centralized matching with the security of onchain settlement. You always maintain custody of your funds.

## Price Discovery

When a new market launches, there's no initial price. The first price emerges when:

1. Someone places a limit order to buy Yes at a price (e.g., `$0.60`)
2. Someone places a limit order to buy No at the complementary price (e.g., `$0.40`)
3. Since `$0.60` + `$0.40` = `$1.00`, the orders match

When matched, `$1.00` is converted into 1 Yes token and 1 No token, each going to their respective buyers.

## Next Steps

<Note>
  Polymarket's orderbook has **no trading size limits** — it matches willing
  buyers and sellers of any amount. However, large orders may move the price
  significantly. Always check orderbook depth before trading in size.
</Note>

<CardGroup cols={2}>
  <Card title="Positions & Tokens" icon="coins" href="/concepts/positions-tokens">
    Learn about outcome tokens and how positions work.
  </Card>

  <Card title="Order Lifecycle" icon="arrows-spin" href="/concepts/order-lifecycle">
    Understand what happens from order placement to settlement.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Positions & Tokens

> Understanding outcome tokens and how positions work on Polymarket

Every prediction on Polymarket is represented by **outcome tokens**. When you trade, you're buying and selling these tokens. Your **position** is simply your balance of tokens for a given market.

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/token-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=cad279109c43c68c541123c2d348c4c5" alt="" className="dark:hidden" width="1596" height="952" data-path="images/core-concepts/token-lifecycle.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/token-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=204d16c1d89892a3c8573060aa04780e" alt="" className="hidden dark:block" width="1596" height="952" data-path="images/dark/core-concepts/token-lifecycle.png" />
</Frame>

## Outcome Tokens

Each market has exactly two outcome tokens:

| Token   | Redeems for | If...                    |
| ------- | ----------- | ------------------------ |
| **Yes** | \$1.00      | The event occurs         |
| **No**  | \$1.00      | The event does not occur |

Tokens are **ERC1155** assets on Polygon, using the [Gnosis Conditional Token Framework](https://github.com/gnosis/conditional-tokens-contracts/) (CTF). This means they're fully onchain and function as standard ERC1155 tokens.

<Note>
  Outcome tokens are always fully backed. Every Yes/No pair in existence is
  backed by exactly `$1` of USDC.e collateral locked in the CTF contract.
</Note>

### Split

Convert USDC.e into outcome tokens. Splitting \$1 creates 1 Yes token and 1 No token.

```
$100 USDC.e → 100 Yes tokens + 100 No tokens
```

Use this when you want to:

* Create inventory for market making
* Obtain both sides of a market

### Trade

Buy or sell tokens on the order book. This is how most users acquire positions.

* **Buy Yes** at `$0.60` → Pay `$0.60`, receive 1 Yes token
* **Sell Yes** at `$0.60` → Give up 1 Yes token, receive `$0.60`

You can sell your position at any time before resolution.

### Merge

Convert a complete set of tokens back into USDC.e. Merging requires equal amounts of Yes and No tokens.

```
100 Yes tokens + 100 No tokens → $100 USDC.e
```

Use this when you want to:

* Exit a position without trading
* Convert accumulated tokens back to collateral

### Redeem

After a market resolves, exchange winning tokens for USDC.e.

| Outcome             | Yes tokens     | No tokens      |
| ------------------- | -------------- | -------------- |
| Event occurs        | Worth \$1 each | Worth \$0      |
| Event doesn't occur | Worth \$0      | Worth \$1 each |

```
100 winning tokens → $100 USDC.e
```

### Position Value

The value of your position depends on the current market price:

```
Position value = Token balance × Current price
```

If you hold 100 Yes tokens and Yes is trading at \$0.75:

```
Position value = 100 × $0.75 = $75
```

## Profit and Loss

Your profit depends on how the market resolves compared to your entry price.

### Example - Buying Yes at 0.40

| Scenario            | Outcome  | Return | Profit                    |
| ------------------- | -------- | ------ | ------------------------- |
| Event occurs        | Yes wins | \$1.00 | +\$0.60 per token (150%)  |
| Event doesn't occur | No wins  | \$0.00 | -\$0.40 per token (-100%) |

### Holding Rewards

Polymarket pays a **4.00% annualized** Holding Reward based on your total position value in eligible markets. Your total position value is randomly sampled once each hour, and the reward is distributed daily. The rate is variable and subject to change at Polymarket's discretion.

### Example - Selling Before Resolution

You can lock in profits or cut losses by selling before the market resolves:

* Bought Yes at `$0.40`
* Price rises to `$0.70`
* Sell at `$0.70` → Profit of `$0.30` per token (75%)


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Order Lifecycle

> Understanding how orders flow from creation to settlement

Every trade on Polymarket follows a specific lifecycle. Orders are created offchain, matched by an operator, and settled onchain through smart contracts. This hybrid approach combines the speed of centralized matching with the security of blockchain settlement.

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/order-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=4db07008193421bfe359afe44b5f604e" alt="" className="dark:hidden" width="2336" height="952" data-path="images/core-concepts/order-lifecycle.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/order-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=5a0f3eba2f20c44471bae05c0670de4a" alt="" className="hidden dark:block" width="2336" height="952" data-path="images/dark/core-concepts/order-lifecycle.png" />
</Frame>

## How Orders Work

All orders on Polymarket are **limit orders**. A limit order specifies the price you're willing to pay (or accept) and the quantity you want to trade.

<Note>
  "Market orders" are simply limit orders with a price set to execute
  immediately against the best available resting orders.
</Note>

Orders are **EIP712-signed messages**. When you place an order, you sign a structured message with your private key. This signature authorizes the Exchange contract to execute the trade on your behalf—without ever taking custody of your funds.

## Order Types

| Type    | Behavior                                                      | Use Case                 |
| ------- | ------------------------------------------------------------- | ------------------------ |
| **GTC** | Good Till Cancelled — rests on book until filled or cancelled | Standard limit orders    |
| **GTD** | Good Till Date — auto-expires at specified time               | Time-limited orders      |
| **FOK** | Fill Or Kill — fill entirely or cancel immediately            | All-or-nothing execution |
| **FAK** | Fill And Kill — fill what's available, cancel the rest        | Partial fills acceptable |

### Post-Only Orders

Post-only orders will only rest on the book. If a post-only order would match immediately (cross the spread), it's rejected instead of executed. This guarantees you're always the maker, never the taker.

<Steps>
  <Step title="Create and Sign">
    Your client creates an order object containing:

    * Token ID (which outcome you're trading)
    * Side (buy or sell)
    * Price and size
    * Expiration time
    * Nonce (for replay protection)

    You sign this order with your private key, creating an EIP712 signature.
  </Step>

  <Step title="Submit to CLOB">
    The signed order is submitted to the Central Limit Order Book (CLOB) operator. The operator validates:

    * Signature is valid
    * You have sufficient balance
    * You have set the required allowances
    * Price meets minimum tick size requirements
  </Step>

  <Step title="Match or Rest">
    **If the order is marketable** (your buy price ≥ lowest ask, or your sell price ≤ highest bid), it matches immediately against resting orders.

    **If the order is not marketable**, it rests on the book waiting for a counterparty. It remains open until:

    * Another order matches against it
    * You cancel it
    * It expires (GTD orders only)
  </Step>

  <Step title="Settlement">
    When orders match, the operator submits the trade to the blockchain. The Exchange contract:

    * Verifies both signatures
    * Transfers tokens from seller to buyer
    * Transfers USDC.e from buyer to seller

    Settlement is **atomic**—either the entire trade succeeds or nothing happens.
  </Step>

  <Step title="Confirmation">
    The trade achieves finality on Polygon. Your token balances update and the trade appears in your history.
  </Step>
</Steps>

## Order Statuses

When you place an order, it receives one of these statuses:

| Status      | Description                                                                 |
| ----------- | --------------------------------------------------------------------------- |
| `live`      | Order is resting on the book                                                |
| `matched`   | Order matched immediately                                                   |
| `delayed`   | Marketable order subject to a 3-second matching delay (sports markets)      |
| `unmatched` | Marketable order placed on the book after the delay expired without a match |

## Trade Statuses

After matching, trades progress through these statuses:

| Status      | Terminal | Description                                            |
| ----------- | -------- | ------------------------------------------------------ |
| `MATCHED`   | No       | Trade matched, sent to executor for onchain submission |
| `MINED`     | No       | Transaction mined into the blockchain                  |
| `CONFIRMED` | Yes      | Trade achieved finality, successful                    |
| `RETRYING`  | No       | Transaction failed, being retried                      |
| `FAILED`    | Yes      | Trade failed permanently                               |

## Maker vs Taker

| Role      | Description                     | When                                                  |
| --------- | ------------------------------- | ----------------------------------------------------- |
| **Maker** | Adds liquidity to the book      | Your order rests and is later matched                 |
| **Taker** | Removes liquidity from the book | Your order matches immediately against resting orders |

Price improvement always benefits the taker. If you place a buy order at `$0.55` and it matches against a resting sell at `$0.52`, you pay `$0.52`.

## Cancellation

You can cancel orders at any time before they're matched:

* **Via API** — Cancel through the CLOB API (instant)
* **Onchain** — Cancel directly on the Exchange contract (fallback if API is unavailable)

Partial fills cannot be cancelled—only the unfilled portion of an order can be cancelled.

## Requirements

Before placing orders, ensure:

| Requirement         | Description                                        |
| ------------------- | -------------------------------------------------- |
| **Balance**         | Sufficient USDC.e (for buys) or tokens (for sells) |
| **Allowance**       | Approve the Exchange contract to spend your assets |
| **API Credentials** | Valid API key for authenticated endpoints          |

<Info>
  Order size is limited by your available balance minus any amounts reserved by existing open orders.

  $$
  \text{maxOrderSize} = \text{balance} - \sum(\text{openOrderSize} - \text{filledAmount})
  $$
</Info>

## Next Steps

<CardGroup cols={2}>
  <Card title="Resolution" icon="gavel" href="/concepts/resolution">
    Learn how markets are resolved and winning tokens redeemed.
  </Card>

  <Card title="Trading Guide" icon="book" href="/trading/overview">
    Start placing orders with our step-by-step guide.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Resolution

> How markets are resolved and winning positions redeemed

When the outcome of an event becomes known, the market is **resolved**. Resolution determines which outcome won, allowing holders of winning tokens to redeem them for \$1 each. Losing tokens become worthless.

Polymarket uses the **UMA Optimistic Oracle** for decentralized, permissionless resolution. Anyone can propose an outcome, and anyone can dispute it if they believe it's incorrect.

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/resolution-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=6726569af3efd6f4fda54528c8eb0d0a" alt="" className="dark:hidden" width="1722" height="952" data-path="images/core-concepts/resolution-lifecycle.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/resolution-lifecycle.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=36e91c655f7f50b18dea3a23b44f8c23" alt="" className="hidden dark:block" width="1722" height="952" data-path="images/dark/core-concepts/resolution-lifecycle.png" />
</Frame>

## Resolution Rules

Every market has pre-defined resolution rules that specify:

* **Resolution source** — Where the outcome will be determined from (e.g., official announcements, specific websites)
* **End date** — When the market is eligible for resolution
* **Edge cases** — How ambiguous situations should be handled

<Warning>
  Always read the resolution rules before trading. The market title describes
  the question, but the **rules** define how it resolves.
</Warning>

<Steps>
  <Step title="Proposal">
    Anyone can propose a resolution by:

    1. Selecting the winning outcome
    2. Posting a bond (typically \$750 USDC.e)
    3. Submitting the proposal to the UMA Oracle

    If the proposal is correct and undisputed, the proposer receives their bond back plus a reward.

    <Warning>
      If you propose incorrectly or too early, you lose your entire bond. Only
      propose if you're confident in the outcome and understand the process.
    </Warning>
  </Step>

  <Step title="Challenge Period">
    After a proposal, there's a **2-hour challenge period** where anyone can dispute the outcome.

    * **If no dispute**: The proposal is accepted and the market resolves
    * **If disputed**: A new proposal round begins. If the second proposal is also disputed, the resolution escalates to UMA's DVM (Data Verification Mechanism) for a token holder vote.

    There are three possible resolution flows:

    1. **No dispute** — Propose then Resolve (fastest, \~2 hours)
    2. **One dispute** — Propose, Challenge, second Propose, Resolve (second proposal accepted)
    3. **Two disputes** — Propose, Challenge, second Propose, second Challenge, Resolve via DVM vote
  </Step>

  <Step title="Dispute - If Challenged">
    To dispute a proposal:

    1. Post a counter-bond (same amount as proposer, typically \$750)
    2. The dispute triggers a new proposal round, or if already in the second round, a debate period

    During the **24-48 hour debate period**, evidence can be submitted in UMA's Discord channels (`#evidence-rationale` and `#voting-discussion`).
  </Step>

  <Step title="UMA Vote">
    After the debate period, UMA token holders vote on the correct outcome. The voting process takes approximately 48 hours.

    | Outcome           | Result                                 | Bond Distribution                                                                                        |
    | ----------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------- |
    | **Proposer wins** | Original proposal accepted             | Proposer gets bond back + half of disputer's bond                                                        |
    | **Disputer wins** | Proposal rejected, new proposal needed | Disputer gets bond back + half of proposer's bond                                                        |
    | **Too Early**     | Event hasn't concluded yet             | Disputer gets bond back + half of proposer's bond                                                        |
    | **Unknown/50-50** | Neither outcome applicable (rare)      | Market resolves 50/50 — each token redeems for \$0.50; disputer gets bond back + half of proposer's bond |
  </Step>
</Steps>

## After Resolution

Once a market resolves:

* **Trading stops** — You can no longer buy or sell tokens for this market
* **Winning tokens** become redeemable for \$1.00 each
* **Losing tokens** become worthless (\$0.00)

### Redeeming Tokens

After resolution, call the `redeemPositions` function on the CTF contract to exchange winning tokens for USDC.e. The contract burns your tokens and returns the corresponding collateral.

```
100 winning tokens → $100 USDC.e
```

## Clarifications

In rare cases, unforeseen circumstances require clarification of the rules after trading begins. Polymarket may issue an **"Additional context"** update that proposers and voters should consider during resolution.

Clarifications:

* Cannot change the fundamental intent of the question
* Are published onchain via the bulletin board contract
* Should be considered by UMA voters when resolving disputes

<Tip>
  If you believe a clarification is needed, request it in the [Polymarket
  Discord](https://discord.com/invite/polymarket) `#market-review` channel.
</Tip>

## Resolution Timeline

| Phase                       | Duration    |
| --------------------------- | ----------- |
| Challenge period            | 2 hours     |
| Debate period (if disputed) | 24-48 hours |
| UMA voting (if disputed)    | \~48 hours  |

**Undisputed resolution**: \~2 hours after proposal

**Disputed resolution**: 4-6 days total

## Contract Addresses

| Contract               | Address                                      | Network         |
| ---------------------- | -------------------------------------------- | --------------- |
| **UmaCtfAdapter v3.0** | `0x157Ce2d672854c848c9b79C49a8Cc6cc89176a49` | Polygon Mainnet |
| **UmaCtfAdapter v2.0** | `0x6A9D222616C90FcA5754cd1333cFD9b7fb6a4F74` | Polygon Mainnet |
| **UmaCtfAdapter v1.0** | `0xCB1822859cEF82Cd2Eb4E6276C7916e692995130` | Polygon Mainnet |

## Resources

* [UMA Oracle Portal](https://oracle.uma.xyz/) — View and interact with proposals
* [UMA Documentation](https://docs.uma.xyz/) — Learn more about the Optimistic Oracle
* [Polymarket Discord](https://discord.com/invite/polymarket) — Discuss resolutions and request clarifications
* [UmaCtfAdapter Source Code](https://github.com/Polymarket/uma-ctf-adapter) — Smart contract source
* [UmaCtfAdapter Audit](https://github.com/Polymarket/uma-ctf-adapter/blob/main/audit/Polymarket_UMA_Optimistic_Oracle_Adapter_Audit.pdf) — Security audit report

## Next Steps

<CardGroup cols={2}>
  <Card title="Positions & Tokens" icon="coins" href="/concepts/positions-tokens">
    Learn how to redeem winning tokens after resolution.
  </Card>

  <Card title="Markets & Events" icon="calendar" href="/concepts/markets-events">
    Understand how markets are structured.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Markets & Events

> Understanding the fundamental building blocks of Polymarket

Every prediction on Polymarket is structured around two core concepts: **markets** and **events**. Understanding how they relate is essential for building on the platform.

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/event-market.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=4c62bd08a405868307cdd6799b368ca5" alt="" className="dark:hidden" width="1540" height="952" data-path="images/core-concepts/event-market.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/event-market.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=2eb5c9b0f8a2afe52bc2e717b7b796a2" alt="" className="hidden dark:block" width="1540" height="952" data-path="images/dark/core-concepts/event-market.png" />
</Frame>

## Markets

A **market** is the fundamental tradable unit on Polymarket. Each market represents a single binary question with Yes/No outcomes.

<Frame>
  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/core-concepts/event.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=0c9a264aec9a22ce5a20c4cc7980806d" alt="" className="dark:hidden" width="1540" height="952" data-path="images/core-concepts/event.png" />

  <img src="https://mintcdn.com/polymarket-292d1b1b/FOMte3ewbG-LVy3k/images/dark/core-concepts/event.png?fit=max&auto=format&n=FOMte3ewbG-LVy3k&q=85&s=912e41bebfe8c1a43ef53b89685ca3d2" alt="" className="hidden dark:block" width="1540" height="952" data-path="images/dark/core-concepts/event.png" />
</Frame>

Every market has:

| Identifier       | Description                                                              |
| ---------------- | ------------------------------------------------------------------------ |
| **Condition ID** | Unique identifier for the market's condition in the CTF contracts        |
| **Question ID**  | Hash of the market question used for resolution                          |
| **Token IDs**    | ERC1155 token IDs used for trading on the CLOB — one for Yes, one for No |

<Note>
  Markets can only be traded via the CLOB if `enableOrderBook` is `true`. Some
  markets may exist onchain but not be available for order book trading.
</Note>

### Market Example

A simple market might be:

> **"Will Bitcoin reach \$150,000 by December 2026?"**

This creates two outcome tokens:

* **Yes token** - Redeemable for `$1` if Bitcoin reaches `$150k`
* **No token** - Redeemable for `$1` if Bitcoin doesn't reach `$100k`

## Events

An **event** is a container that groups one or more related markets together. Events provide organizational structure and enable multi-outcome predictions.

### Single-Market Events

When an event contains just one market, it creates a simple market pair. The event and market are essentially equivalent.

```
Event: Will Bitcoin reach $100,000 by December 2024?
└── Market: Will Bitcoin reach $100,000 by December 2024? (Yes/No)
```

### Multi-Market Events

When an event contains two or more markets, it creates a grouped market pair. This enables mutually exclusive multi-outcome predictions.

```
Event: Who will win the 2024 Presidential Election?
├── Market: Donald Trump? (Yes/No)
├── Market: Joe Biden? (Yes/No)
├── Market: Kamala Harris? (Yes/No)
└── Market: Other? (Yes/No)
```

## Identifying Markets

Every market and event has a unique **slug** that appears in the Polymarket URL:

```
https://polymarket.com/event/fed-decision-in-october
                              └── slug: fed-decision-in-october
```

You can use slugs to fetch specific markets or events from the API:

```bash  theme={null}
# Fetch event by slug
curl "https://gamma-api.polymarket.com/events?slug=fed-decision-in-october"
```

## Sports Markets

Specifically for sports markets, outstanding limit orders are **automatically cancelled** once the game begins, clearing the order book at the official start time. However, game start times can shift — if a game starts earlier than scheduled, orders may not be cleared in time. Always monitor your orders closely around game start times.

***

## Next Steps

<CardGroup cols={2}>
  <Card title="Prices & Orderbook" icon="chart-line" href="/concepts/prices-orderbook">
    Learn how prices are determined and how the order book works.
  </Card>

  <Card title="Fetching Market Data" icon="code" href="/market-data/overview">
    Start querying markets and events from the API.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Fetching Markets

> Three strategies for discovering and querying markets

<Tip>
  Both the events and markets endpoints are paginated. See
  [pagination](#pagination) for details.
</Tip>

There are three main strategies for retrieving market data, each optimized for different use cases:

1. **By Slug** — Best for fetching specific individual markets or events
2. **By Tags** — Ideal for filtering markets by category or sport
3. **Via Events Endpoint** — Most efficient for retrieving all active markets

***

## Fetch by Slug

**Use case:** When you need to retrieve a specific market or event that you already know about.

Individual markets and events are best fetched using their unique slug identifier. The slug can be found directly in the Polymarket frontend URL.

### How to Extract the Slug

From any Polymarket URL, the slug is the path segment after `/event/`:

```
https://polymarket.com/event/fed-decision-in-october
                                ↑
                      Slug: fed-decision-in-october
```

### Examples

```bash  theme={null}
# Fetch an event by slug (query parameter)
curl "https://gamma-api.polymarket.com/events?slug=fed-decision-in-october"

# Or use the path endpoint
curl "https://gamma-api.polymarket.com/events/slug/fed-decision-in-october"
```

```bash  theme={null}
# Fetch a market by slug (query parameter)
curl "https://gamma-api.polymarket.com/markets?slug=fed-decision-in-october"

# Or use the path endpoint
curl "https://gamma-api.polymarket.com/markets/slug/fed-decision-in-october"
```

***

## Fetch by Tags

**Use case:** When you want to filter markets by category, sport, or topic.

Tags provide a way to categorize and filter markets. You can discover available tags and then use them to filter your requests.

### Discover Available Tags

**General tags:** `GET /tags` (Gamma API)

**Sports tags and metadata:** `GET /sports` (Gamma API)

The `/sports` endpoint returns metadata for sports including tag IDs, images, resolution sources, and series information.

### Filter by Tag

Once you have tag IDs, use the `tag_id` parameter in both events and markets endpoints:

```bash  theme={null}
# Fetch events for a specific tag
curl "https://gamma-api.polymarket.com/events?tag_id=100381&limit=10&active=true&closed=false"
```

### Additional Tag Filtering

You can also:

* Use `related_tags=true` to include related tag markets
* Exclude specific tags with `exclude_tag_id`

```bash  theme={null}
# Include related tags
curl "https://gamma-api.polymarket.com/events?tag_id=100381&related_tags=true&active=true&closed=false"
```

***

## Fetch All Active Markets

**Use case:** When you need to retrieve all available active markets, typically for broader analysis or market discovery.

The most efficient approach is to use the events endpoint with `active=true&closed=false`, as events contain their associated markets.

```bash  theme={null}
curl "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=100"
```

### Key Parameters

| Parameter   | Description                                                                                                      |
| ----------- | ---------------------------------------------------------------------------------------------------------------- |
| `order`     | Field to order by (`volume_24hr`, `volume`, `liquidity`, `start_date`, `end_date`, `competitive`, `closed_time`) |
| `ascending` | Sort direction (`true` for ascending, `false` for descending). Default: `false`                                  |
| `active`    | Filter by active status (`true` for live tradable events)                                                        |
| `closed`    | Filter by closed status                                                                                          |
| `limit`     | Results per page                                                                                                 |
| `offset`    | Number of results to skip for pagination                                                                         |

```bash  theme={null}
# Get the highest volume active events
curl "https://gamma-api.polymarket.com/events?active=true&closed=false&order=volume_24hr&ascending=false&limit=100"
```

***

## Pagination

All list endpoints return paginated responses with `limit` and `offset` parameters:

```bash  theme={null}
# Page 1: First 50 results
curl "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=50&offset=0"

# Page 2: Next 50 results
curl "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=50&offset=50"

# Page 3: Next 50 results
curl "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=50&offset=100"
```

***

## Best Practices

1. **For individual markets:** Use the slug method for direct lookups
2. **For category browsing:** Use tag filtering to reduce API calls
3. **For complete market discovery:** Use the events endpoint with pagination
4. **Always include `active=true&closed=false`** unless you specifically need historical data
5. **Use the events endpoint** and work backwards — events contain their associated markets, reducing the number of API calls needed

***

## Next Steps

<CardGroup cols={2}>
  <Card title="API Reference" icon="code" href="/api-reference/introduction">
    Full endpoint documentation with parameters and response schemas.
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.polymarket.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Orderbook

> Reading the orderbook, prices, spreads, and midpoints

The orderbook is a public endpoint — no authentication required. You can read prices and liquidity using the SDK or REST API directly.

<CodeGroup>
  ```typescript TypeScript theme={null}
  import { ClobClient } from "@polymarket/clob-client";

  const client = new ClobClient("https://clob.polymarket.com", 137);
  ```

  ```python Python theme={null}
  from py_clob_client.client import ClobClient

  client = ClobClient("https://clob.polymarket.com", chain_id=137)
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::Client;

  let client = Client::default(); // https://clob.polymarket.com
  ```

  ```bash REST theme={null}
  # Base URL for all orderbook endpoints
  https://clob.polymarket.com
  ```
</CodeGroup>

***

## Get the Orderbook

Fetch the full orderbook for a token, including all resting bid and ask levels:

<CodeGroup>
  ```typescript TypeScript theme={null}
  const book = await client.getOrderBook("TOKEN_ID");

  console.log("Best bid:", book.bids[0]);
  console.log("Best ask:", book.asks[0]);
  console.log("Tick size:", book.tick_size);
  ```

  ```python Python theme={null}
  book = client.get_order_book("TOKEN_ID")

  print("Best bid:", book["bids"][0])
  print("Best ask:", book["asks"][0])
  print("Tick size:", book["tick_size"])
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::request::OrderBookSummaryRequest;

  let token_id = "TOKEN_ID".parse()?;
  let request = OrderBookSummaryRequest::builder().token_id(token_id).build();
  let book = client.order_book(&request).await?;

  println!("Best bid: {:?}", book.bids[0]);
  println!("Best ask: {:?}", book.asks[0]);
  println!("Tick size: {:?}", book.tick_size);
  ```

  ```bash REST theme={null}
  curl "https://clob.polymarket.com/book?token_id=TOKEN_ID"
  ```
</CodeGroup>

### Response

```json  theme={null}
{
  "market": "0xbd31dc8a...",
  "asset_id": "52114319501245...",
  "timestamp": "2023-10-21T08:00:00Z",
  "bids": [
    { "price": "0.48", "size": "1000" },
    { "price": "0.47", "size": "2500" }
  ],
  "asks": [
    { "price": "0.52", "size": "800" },
    { "price": "0.53", "size": "1500" }
  ],
  "min_order_size": "5",
  "tick_size": "0.01",
  "neg_risk": false,
  "hash": "0xabc123..."
}
```

| Field            | Description                                         |
| ---------------- | --------------------------------------------------- |
| `market`         | Condition ID of the market                          |
| `asset_id`       | Token ID                                            |
| `bids`           | Buy orders sorted by price (highest first)          |
| `asks`           | Sell orders sorted by price (lowest first)          |
| `tick_size`      | Minimum price increment for this market             |
| `min_order_size` | Minimum order size for this market                  |
| `neg_risk`       | Whether this is a multi-outcome (neg risk) market   |
| `hash`           | Hash of the orderbook state — use to detect changes |

***

## Prices

Get the best available price for buying or selling a token:

<CodeGroup>
  ```typescript TypeScript theme={null}
  const buyPrice = await client.getPrice("TOKEN_ID", "BUY");
  console.log("Best ask:", buyPrice.price); // Price you'd pay to buy

  const sellPrice = await client.getPrice("TOKEN_ID", "SELL");
  console.log("Best bid:", sellPrice.price); // Price you'd receive to sell
  ```

  ```python Python theme={null}
  buy_price = client.get_price("TOKEN_ID", "BUY")
  print("Best ask:", buy_price["price"])

  sell_price = client.get_price("TOKEN_ID", "SELL")
  print("Best bid:", sell_price["price"])
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::{Side, request::PriceRequest};

  let token_id = "TOKEN_ID".parse()?;

  let buy_req = PriceRequest::builder().token_id(token_id).side(Side::Buy).build();
  let buy_price = client.price(&buy_req).await?;
  println!("Best ask: {}", buy_price.price);

  let sell_req = PriceRequest::builder().token_id(token_id).side(Side::Sell).build();
  let sell_price = client.price(&sell_req).await?;
  println!("Best bid: {}", sell_price.price);
  ```

  ```bash REST theme={null}
  # Best price for buying (lowest ask)
  curl "https://clob.polymarket.com/price?token_id=TOKEN_ID&side=BUY"

  # Best price for selling (highest bid)
  curl "https://clob.polymarket.com/price?token_id=TOKEN_ID&side=SELL"
  ```
</CodeGroup>

***

## Midpoints

The midpoint is the average of the best bid and best ask. This is the price displayed on Polymarket as the market's implied probability.

<CodeGroup>
  ```typescript TypeScript theme={null}
  const midpoint = await client.getMidpoint("TOKEN_ID");
  console.log("Midpoint:", midpoint.mid); // e.g., "0.50"
  ```

  ```python Python theme={null}
  midpoint = client.get_midpoint("TOKEN_ID")
  print("Midpoint:", midpoint["mid"])
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::request::MidpointRequest;

  let token_id = "TOKEN_ID".parse()?;
  let request = MidpointRequest::builder().token_id(token_id).build();
  let midpoint = client.midpoint(&request).await?;
  println!("Midpoint: {}", midpoint.mid);
  ```

  ```bash REST theme={null}
  curl "https://clob.polymarket.com/midpoint?token_id=TOKEN_ID"
  ```
</CodeGroup>

<Note>
  If the bid-ask spread is wider than \$0.10, Polymarket displays the last traded
  price instead of the midpoint.
</Note>

***

## Spreads

The spread is the difference between the best ask and the best bid. Tighter spreads indicate more liquid markets.

<CodeGroup>
  ```typescript TypeScript theme={null}
  const spread = await client.getSpread("TOKEN_ID");
  console.log("Spread:", spread.spread); // e.g., "0.04"
  ```

  ```python Python theme={null}
  spread = client.get_spread("TOKEN_ID")
  print("Spread:", spread["spread"])
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::request::SpreadRequest;

  let token_id = "TOKEN_ID".parse()?;
  let request = SpreadRequest::builder().token_id(token_id).build();
  let spread = client.spread(&request).await?;
  println!("Spread: {}", spread.spread);
  ```

  ```bash REST theme={null}
  # Spreads use POST for batch requests
  curl -X POST "https://clob.polymarket.com/spreads" \
    -H "Content-Type: application/json" \
    -d '[{"token_id": "TOKEN_ID"}]'
  ```
</CodeGroup>

***

## Price History

Fetch historical price data for a token over various time intervals:

<CodeGroup>
  ```typescript TypeScript theme={null}
  import { PriceHistoryInterval } from "@polymarket/clob-client";

  const history = await client.getPricesHistory({
    market: "TOKEN_ID", // Note: this param is named "market" but takes a token ID
    interval: PriceHistoryInterval.ONE_DAY,
    fidelity: 60, // Data points every 60 minutes
  });

  // Each entry: { t: timestamp, p: price }
  history.forEach((point) => {
    console.log(`${new Date(point.t * 1000).toISOString()}: ${point.p}`);
  });
  ```

  ```python Python theme={null}
  history = client.get_prices_history(
      market="TOKEN_ID",  # Note: this param is named "market" but takes a token ID
      interval="1d",
      fidelity=60,  # Data points every 60 minutes
  )

  for point in history:
      print(f"{point['t']}: {point['p']}")
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::{Interval, TimeRange, request::PriceHistoryRequest};

  let token_id = "TOKEN_ID".parse()?;
  let request = PriceHistoryRequest::builder()
      .market(token_id) // Note: this param is named "market" but takes a token ID
      .time_range(TimeRange::Interval { interval: Interval::OneDay })
      .fidelity(60) // Data points every 60 minutes
      .build();
  let history = client.price_history(&request).await?;

  for point in &history.history {
      println!("{}: {}", point.t, point.p);
  }
  ```

  ```bash REST theme={null}
  # By interval (relative to now)
  curl "https://clob.polymarket.com/prices-history?market=TOKEN_ID&interval=1d&fidelity=60"

  # By timestamp range
  curl "https://clob.polymarket.com/prices-history?market=TOKEN_ID&startTs=1697875200&endTs=1697961600"
  ```
</CodeGroup>

| Interval | Description        |
| -------- | ------------------ |
| `1h`     | Last hour          |
| `6h`     | Last 6 hours       |
| `1d`     | Last day           |
| `1w`     | Last week          |
| `1m`     | Last month         |
| `max`    | All available data |

<Note>
  `interval` is relative to the current time. Use `startTs` / `endTs` for
  absolute time ranges. They are mutually exclusive — don't combine them.
</Note>

***

## Estimate Fill Price

Calculate the effective price you'd pay for a market order of a given size, accounting for orderbook depth:

<CodeGroup>
  ```typescript TypeScript theme={null}
  import { Side, OrderType } from "@polymarket/clob-client";

  // What price would I pay to buy $500 worth?
  const price = await client.calculateMarketPrice(
    "TOKEN_ID",
    Side.BUY,
    500, // dollar amount
    OrderType.FOK,
  );

  console.log("Estimated fill price:", price);
  ```

  ```python Python theme={null}
  from py_clob_client.clob_types import OrderType

  price = client.calculate_market_price(
      token_id="TOKEN_ID",
      side="BUY",
      amount=500,
      order_type=OrderType.FOK,
  )

  print("Estimated fill price:", price)
  ```

  ```rust Rust theme={null}
  // The Rust SDK handles market price calculation automatically
  // inside the market_order() builder when no price is specified.
  // It walks the orderbook to determine the fill price for you.
  let order = client
      .market_order()
      .token_id("TOKEN_ID".parse()?)
      .amount(Amount::usdc(dec!(500))?)
      .side(Side::Buy)
      .order_type(OrderType::FOK)
      .build()
      .await?; // Price auto-calculated from orderbook depth
  ```
</CodeGroup>

This walks the orderbook to estimate slippage. Useful for sizing market orders before submitting them.

***

## Batch Requests

All orderbook queries have batch variants for fetching data across multiple tokens in a single request (up to 500 tokens):

| Single                | Batch                   | REST              |
| --------------------- | ----------------------- | ----------------- |
| `getOrderBook()`      | `getOrderBooks()`       | `POST /books`     |
| `getPrice()`          | `getPrices()`           | `POST /prices`    |
| `getMidpoint()`       | `getMidpoints()`        | `POST /midpoints` |
| `getSpread()`         | `getSpreads()`          | `POST /spreads`   |
| `getLastTradePrice()` | `getLastTradesPrices()` | —                 |

<Note>
  `BookParams` for batch orderbook requests accepts a `token_id` and an optional
  `side` parameter to filter by bid or ask side.
</Note>

<CodeGroup>
  ```typescript TypeScript theme={null}
  import { Side } from "@polymarket/clob-client";

  // Fetch prices for multiple tokens
  const prices = await client.getPrices([
    { token_id: "TOKEN_A", side: Side.BUY },
    { token_id: "TOKEN_B", side: Side.BUY },
  ]);
  // Returns: { "TOKEN_A": { "BUY": "0.52" }, "TOKEN_B": { "BUY": "0.74" } }
  ```

  ```python Python theme={null}
  prices = client.get_prices([
      {"token_id": "TOKEN_A", "side": "BUY"},
      {"token_id": "TOKEN_B", "side": "BUY"},
  ])
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::{Side, request::PriceRequest};

  let token_a = "TOKEN_A".parse()?;
  let token_b = "TOKEN_B".parse()?;
  let requests = vec![
      PriceRequest::builder().token_id(token_a).side(Side::Buy).build(),
      PriceRequest::builder().token_id(token_b).side(Side::Buy).build(),
  ];
  let prices = client.prices(&requests).await?;
  ```

  ```bash REST theme={null}
  curl -X POST "https://clob.polymarket.com/prices" \
    -H "Content-Type: application/json" \
    -d '[
      {"token_id": "TOKEN_A", "side": "BUY"},
      {"token_id": "TOKEN_B", "side": "BUY"}
    ]'
  ```
</CodeGroup>

***

## Last Trade Price

Get the price and side of the most recent trade for a token:

<CodeGroup>
  ```typescript TypeScript theme={null}
  const lastTrade = await client.getLastTradePrice("TOKEN_ID");
  console.log(lastTrade.price, lastTrade.side);
  // e.g., "0.52", "BUY"
  ```

  ```python Python theme={null}
  last_trade = client.get_last_trade_price("TOKEN_ID")
  print(last_trade["price"], last_trade["side"])
  ```

  ```rust Rust theme={null}
  use polymarket_client_sdk::clob::types::request::LastTradePriceRequest;

  let token_id = "TOKEN_ID".parse()?;
  let request = LastTradePriceRequest::builder().token_id(token_id).build();
  let last_trade = client.last_trade_price(&request).await?;
  println!("{} {:?}", last_trade.price, last_trade.side);
  ```
</CodeGroup>

***

## Real-Time Updates

For live orderbook data, use the WebSocket API instead of polling. The `market` channel streams orderbook changes, price updates, and trade events in real time.

### Connecting

```typescript  theme={null}
const ws = new WebSocket(
  "wss://ws-subscriptions-clob.polymarket.com/ws/market",
);

ws.onopen = () => {
  ws.send(
    JSON.stringify({
      type: "market",
      assets_ids: ["TOKEN_ID"],
      custom_feature_enabled: true, // enables best_bid_ask, new_market, market_resolved events
    }),
  );
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  switch (data.event_type) {
    case "book": // full orderbook snapshot
    case "price_change": // individual price level update
    case "last_trade_price": // new trade executed
    case "tick_size_change": // market tick size changed
    case "best_bid_ask": // top-of-book update (requires custom_feature_enabled)
    case "new_market": // new market created (requires custom_feature_enabled)
    case "market_resolved": // market resolved (requires custom_feature_enabled)
  }
};
```

### Dynamic Subscribe and Unsubscribe

After connecting, you can change your subscriptions without reconnecting:

```typescript  theme={null}
// Subscribe to additional tokens
ws.send(
  JSON.stringify({
    assets_ids: ["NEW_TOKEN_ID"],
    operation: "subscribe",
  }),
);

// Unsubscribe from tokens
ws.send(
  JSON.stringify({
    assets_ids: ["OLD_TOKEN_ID"],
    operation: "unsubscribe",
  }),
);
```

### Event Types

| Event              | Trigger                                      | Key Fields                                                             |
| ------------------ | -------------------------------------------- | ---------------------------------------------------------------------- |
| `book`             | On subscribe + when a trade affects the book | `bids[]`, `asks[]`, `hash`, `timestamp`                                |
| `price_change`     | New order placed or order cancelled          | `price_changes[]` with `price`, `size`, `side`, `best_bid`, `best_ask` |
| `last_trade_price` | Trade executed                               | `price`, `side`, `size`, `fee_rate_bps`                                |
| `tick_size_change` | Price hits >0.96 or \< 0.04                  | `old_tick_size`, `new_tick_size`                                       |
| `best_bid_ask`     | Top-of-book changes                          | `best_bid`, `best_ask`, `spread`                                       |
| `new_market`       | Market created                               | `question`, `assets_ids`, `outcomes`                                   |
| `market_resolved`  | Market resolved                              | `winning_asset_id`, `winning_outcome`                                  |

<Note>
  `best_bid_ask`, `new_market`, and `market_resolved` require
  `custom_feature_enabled: true` in your subscription message.
</Note>

<Warning>
  The `tick_size_change` event is critical for trading bots. If the tick size
  changes and you continue using the old tick size, your orders will be
  rejected.
</Warning>

***

## Next Steps

<CardGroup cols={2}>
  <Card title="Place Orders" icon="plus" href="/trading/orders/create">
    Create and submit orders using the orderbook data
  </Card>

  <Card title="Fetching Markets" icon="magnifying-glass" href="/market-data/fetching-markets">
    Find token IDs for markets you want to trade
  </Card>
</CardGroup>


Built with [Mintlify](https://mintlify.com).

