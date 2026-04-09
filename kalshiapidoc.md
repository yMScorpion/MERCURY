[Skip to main content](https://docs.kalshi.com/getting_started/quick_start_market_data#content-area)

[API Documentation home page![light logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)![dark logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)](https://docs.kalshi.com/)

Search...

Ctrl KAsk AI

Search...

Navigation

Quick Start: Market Data (No SDK)

[Welcome](https://docs.kalshi.com/welcome) [Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) [Concepts](https://docs.kalshi.com/getting_started/making_your_first_request) [REST](https://docs.kalshi.com/api-reference/historical/get-historical-cutoff-timestamps) [Websockets](https://docs.kalshi.com/websockets/websocket-connection) [FIX](https://docs.kalshi.com/fix) [SDKs](https://docs.kalshi.com/sdks/overview) [Changelog](https://docs.kalshi.com/changelog)

- [Quick Start: Market Data (No SDK)](https://docs.kalshi.com/getting_started/quick_start_market_data)

- [Quick Start: Authenticated Requests (No SDK)](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests)

- [Quick Start: Create your first order (No SDK)](https://docs.kalshi.com/getting_started/quick_start_create_order)

- [Quick Start: WebSockets (No SDK)](https://docs.kalshi.com/getting_started/quick_start_websockets)

On this page

- [Making Unauthenticated Requests](https://docs.kalshi.com/getting_started/quick_start_market_data#making-unauthenticated-requests)
- [Step 1: Get Series Information](https://docs.kalshi.com/getting_started/quick_start_market_data#step-1-get-series-information)
- [Step 2: Get Today’s Events and Markets](https://docs.kalshi.com/getting_started/quick_start_market_data#step-2-get-today%E2%80%99s-events-and-markets)
- [Step 3: Get Orderbook Data](https://docs.kalshi.com/getting_started/quick_start_market_data#step-3-get-orderbook-data)
- [Working with Large Datasets](https://docs.kalshi.com/getting_started/quick_start_market_data#working-with-large-datasets)
- [Understanding Orderbook Responses](https://docs.kalshi.com/getting_started/quick_start_market_data#understanding-orderbook-responses)
- [Next Steps](https://docs.kalshi.com/getting_started/quick_start_market_data#next-steps)

# Quick Start: Market Data (No SDK)

Learn how to access real-time market data without authentication

This guide will walk you through accessing Kalshi’s public market data endpoints without authentication. You’ll learn how to retrieve series information, events, markets, and orderbook data for the popular “Who will have a higher net approval” market.

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#making-unauthenticated-requests)  Making Unauthenticated Requests

Kalshi provides several public endpoints that don’t require API keys. These endpoints allow you to access market data directly from our production servers at `https://api.elections.kalshi.com/trade-api/v2`.

**Note about the API URL**: Despite the “elections” subdomain, `api.elections.kalshi.com` provides access to ALL Kalshi markets - not just election-related ones. This includes markets on economics, climate, technology, entertainment, and more.

No authentication headers are required for the endpoints in this guide. You can start making requests immediately!

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#step-1-get-series-information)  Step 1: Get Series Information

Let’s start by fetching information about the KXHIGHNY series ( [Highest temperature in NYC today?](https://kalshi.com/markets/kxhighny/highest-temperature-in-nyc)). This series tracks the highest temperature recorded in Central Park, New York on a given day. We’ll use the [Get Series](https://docs.kalshi.com/api-reference/market/get-series) endpoint.

Python

JavaScript

cURL

```
import requests

# Get series information for KXHIGHNY
url = "https://api.elections.kalshi.com/trade-api/v2/series/KXHIGHNY"
response = requests.get(url)
series_data = response.json()

print(f"Series Title: {series_data['series']['title']}")
print(f"Frequency: {series_data['series']['frequency']}")
print(f"Category: {series_data['series']['category']}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#step-2-get-today%E2%80%99s-events-and-markets)  Step 2: Get Today’s Events and Markets

Now that we have the series information, let’s get the markets for this series. We’ll use the [Get Markets](https://docs.kalshi.com/api-reference/market/get-markets) endpoint with the series ticker filter to find all active markets. If there are no open markets today, remove `status=open` or use `status=all` to see the full series history.

Python

JavaScript

```
# Get all open markets for the KXHIGHNY series
markets_url = f"https://api.elections.kalshi.com/trade-api/v2/markets?series_ticker=KXHIGHNY&status=open"
markets_response = requests.get(markets_url)
markets_data = markets_response.json()

print(f"\nActive markets in KXHIGHNY series:")
for market in markets_data['markets']:
    print(f"- {market['ticker']}: {market['title']}")
    print(f"  Event: {market['event_ticker']}")
    print(f"  Yes Price: ${market['yes_bid_dollars']} | Volume: {market['volume_fp']}")
    print()

# Get details for a specific event if you have its ticker
if markets_data['markets']:
    # Let's get details for the first market's event
    event_ticker = markets_data['markets'][0]['event_ticker']
    event_url = f"https://api.elections.kalshi.com/trade-api/v2/events/{event_ticker}"
    event_response = requests.get(event_url)
    event_data = event_response.json()

    print(f"Event Details:")
    print(f"Title: {event_data['event']['title']}")
    print(f"Category: {event_data['event']['category']}")
```

You can view these markets in the Kalshi UI at: [https://kalshi.com/markets/kxhighny](https://kalshi.com/markets/kxhighny)

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#step-3-get-orderbook-data)  Step 3: Get Orderbook Data

Now let’s fetch the orderbook for a specific market to see the current bids and asks using the [Get Market Orderbook](https://docs.kalshi.com/api-reference/market/get-market-order-book) endpoint. This snippet assumes you still have the `markets_data` from the previous step. If `markets_data['markets']` is empty, pick a market from a different series or remove the `status=open` filter.

Python

JavaScript

```
# Get orderbook for a specific market
# Replace with an actual market ticker from the markets list
if not markets_data['markets']:
    raise ValueError("No open markets found. Try removing status=open or choose another series.")

market_ticker = markets_data['markets'][0]['ticker']
orderbook_url = f"https://api.elections.kalshi.com/trade-api/v2/markets/{market_ticker}/orderbook"

orderbook_response = requests.get(orderbook_url)
orderbook_data = orderbook_response.json()

print(f"\nOrderbook for {market_ticker}:")
print("YES BIDS:")
for price_dollars, count_fp in orderbook_data['orderbook_fp']['yes_dollars'][:5]:  # Show top 5
    print(f"  Price: ${price_dollars}, Quantity: {count_fp}")

print("\nNO BIDS:")
for price_dollars, count_fp in orderbook_data['orderbook_fp']['no_dollars'][:5]:  # Show top 5
    print(f"  Price: ${price_dollars}, Quantity: {count_fp}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#working-with-large-datasets)  Working with Large Datasets

The Kalshi API uses cursor-based pagination to handle large datasets efficiently. To learn more about navigating through paginated responses, see our [Understanding Pagination](https://docs.kalshi.com/getting_started/pagination) guide.

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#understanding-orderbook-responses)  Understanding Orderbook Responses

Kalshi’s orderbook structure is unique due to the nature of binary prediction markets. The API only returns bids (not asks) because of the reciprocal relationship between YES and NO positions. To learn more about orderbook responses and why they work this way, see our [Orderbook Responses](https://docs.kalshi.com/getting_started/orderbook_responses) guide.

## [​](https://docs.kalshi.com/getting_started/quick_start_market_data\#next-steps)  Next Steps

Now that you understand how to access market data without authentication, you can:

1. Explore other public series and events
2. Build real-time market monitoring tools
3. Create market analysis dashboards
4. Set up a WebSocket connection for live updates (requires authentication)

For authenticated endpoints that allow trading and portfolio management, check out our [API Keys guide](https://docs.kalshi.com/getting_started/api_keys).

[Quick Start: Authenticated Requests (No SDK)](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests)

Ctrl+I

[Powered byThis documentation is built and hosted on Mintlify, a developer documentation platform](https://www.mintlify.com/?utm_campaign=poweredBy&utm_medium=referral&utm_source=kalshi-b198743e)

Assistant

Responses are generated using AI and may contain mistakes.

    [Skip to main content](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#content-area)

[API Documentation home page![light logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)![dark logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)](https://docs.kalshi.com/)

Search...

Ctrl KAsk AI

Search...

Navigation

Quick Start: Authenticated Requests (No SDK)

[Welcome](https://docs.kalshi.com/welcome) [Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) [Concepts](https://docs.kalshi.com/getting_started/making_your_first_request) [REST](https://docs.kalshi.com/api-reference/historical/get-historical-cutoff-timestamps) [Websockets](https://docs.kalshi.com/websockets/websocket-connection) [FIX](https://docs.kalshi.com/fix) [SDKs](https://docs.kalshi.com/sdks/overview) [Changelog](https://docs.kalshi.com/changelog)

- [Quick Start: Market Data (No SDK)](https://docs.kalshi.com/getting_started/quick_start_market_data)

- [Quick Start: Authenticated Requests (No SDK)](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests)

- [Quick Start: Create your first order (No SDK)](https://docs.kalshi.com/getting_started/quick_start_create_order)

- [Quick Start: WebSockets (No SDK)](https://docs.kalshi.com/getting_started/quick_start_websockets)

On this page

- [Step 1: Get Your API Keys](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#step-1-get-your-api-keys)
- [Step 2: Set Up Your Request](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#step-2-set-up-your-request)
- [How to Create the Signature](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#how-to-create-the-signature)
- [Step 3: Get Your Balance](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#step-3-get-your-balance)
- [Complete Working Example](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#complete-working-example)
- [Common Issues](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#common-issues)
- [Next Steps](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests#next-steps)

# Quick Start: Authenticated Requests (No SDK)

Three simple steps to make your first authenticated API request to Kalshi

This guide shows you how to make authenticated requests to the Kalshi API in three simple steps.

## [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#step-1-get-your-api-keys)  Step 1: Get Your API Keys

1. Log in to your Kalshi account ( [demo](https://demo.kalshi.co/) or [production](https://kalshi.com/))
2. Navigate to **Account & security** → **API Keys**
3. Click **Create Key**
4. Save both:
   - **Private Key**: Downloaded as a `.key` file
   - **API Key ID**: Displayed on screen (looks like `a952bcbe-ec3b-4b5b-b8f9-11dae589608c`)

Your private key cannot be retrieved after this page is closed. Store it securely!

## [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#step-2-set-up-your-request)  Step 2: Set Up Your Request

Every authenticated request to Kalshi requires three headers:

| Header | Description | Example |
| --- | --- | --- |
| `KALSHI-ACCESS-KEY` | Your API Key ID | `a952bcbe-ec3b-4b5b-b8f9-11dae589608c` |
| `KALSHI-ACCESS-TIMESTAMP` | Current time in milliseconds | `1703123456789` |
| `KALSHI-ACCESS-SIGNATURE` | Request signature (see below) | `base64_encoded_signature` |

### [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#how-to-create-the-signature)  How to Create the Signature

The signature proves you own the private key. Here’s how it works:

1. **Create a message string**: Concatenate `timestamp + HTTP_METHOD + path`   - Example: `1703123456789GET/trade-api/v2/portfolio/balance`
   - **Important**: Use the path **without query parameters**. For `/portfolio/orders?limit=5`, sign only `/trade-api/v2/portfolio/orders`
2. **Sign with your private key**: Use RSA-PSS with SHA256
3. **Encode as base64**: Convert the signature to base64 string

Here’s the signing process in Python:

```
import base64
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding

def sign_request(private_key, timestamp, method, path):
    # Strip query parameters from path before signing
    path_without_query = path.split('?')[0]

    # Create the message to sign
    message = f"{timestamp}{method}{path_without_query}".encode('utf-8')

    # Sign with RSA-PSS
    signature = private_key.sign(
        message,
        padding.PSS(
            mgf=padding.MGF1(hashes.SHA256()),
            salt_length=padding.PSS.DIGEST_LENGTH
        ),
        hashes.SHA256()
    )

    # Return base64 encoded
    return base64.b64encode(signature).decode('utf-8')
```

## [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#step-3-get-your-balance)  Step 3: Get Your Balance

Now let’s make your first authenticated request to get your account balance:

```
import requests
import datetime

# Set up the request
timestamp = str(int(datetime.datetime.now().timestamp() * 1000))
method = "GET"
path = "/trade-api/v2/portfolio/balance"

# Create signature (using function from Step 2)
signature = sign_request(private_key, timestamp, method, path)

# Make the request
headers = {
    'KALSHI-ACCESS-KEY': 'your-api-key-id',
    'KALSHI-ACCESS-SIGNATURE': signature,
    'KALSHI-ACCESS-TIMESTAMP': timestamp
}

response = requests.get('https://demo-api.kalshi.co' + path, headers=headers)
balance = response.json()

print(f"Your balance: ${balance['balance'] / 100:.2f}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#complete-working-example)  Complete Working Example

Here’s the minimal code to get your balance:

```
import requests
import datetime
import base64
from urllib.parse import urlparse
from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives.asymmetric import padding

# Configuration
API_KEY_ID = 'your-api-key-id-here'
PRIVATE_KEY_PATH = 'path/to/your/kalshi-key.key'
BASE_URL = 'https://demo-api.kalshi.co/trade-api/v2'  # or 'https://api.elections.kalshi.com/trade-api/v2' for production

def load_private_key(key_path):
    with open(key_path, "rb") as f:
        return serialization.load_pem_private_key(f.read(), password=None, backend=default_backend())

def create_signature(private_key, timestamp, method, path):
    """Create the request signature."""
    # Strip query parameters before signing
    path_without_query = path.split('?')[0]
    message = f"{timestamp}{method}{path_without_query}".encode('utf-8')
    signature = private_key.sign(
        message,
        padding.PSS(mgf=padding.MGF1(hashes.SHA256()), salt_length=padding.PSS.DIGEST_LENGTH),
        hashes.SHA256()
    )
    return base64.b64encode(signature).decode('utf-8')

def get(private_key, api_key_id, path, base_url=BASE_URL):
    """Make an authenticated GET request to the Kalshi API."""
    timestamp = str(int(datetime.datetime.now().timestamp() * 1000))
    # Signing requires the full URL path from root (e.g. /trade-api/v2/portfolio/balance)
    sign_path = urlparse(base_url + path).path
    signature = create_signature(private_key, timestamp, "GET", sign_path)

    headers = {
        'KALSHI-ACCESS-KEY': api_key_id,
        'KALSHI-ACCESS-SIGNATURE': signature,
        'KALSHI-ACCESS-TIMESTAMP': timestamp
    }

    return requests.get(base_url + path, headers=headers)

# Load private key
private_key = load_private_key(PRIVATE_KEY_PATH)

# Get balance
response = get(private_key, API_KEY_ID, "/portfolio/balance")
print(f"Your balance: ${response.json()['balance'] / 100:.2f}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#common-issues)  Common Issues

| Problem | Solution |
| --- | --- |
| 401 Unauthorized | Check your API Key ID and private key file path |
| Signature error | Ensure timestamp is in milliseconds (not seconds) |
| Path not found | Path includes `/trade-api/v2`, pass only the endpoint path (e.g. `/portfolio/balance`, not `/trade-api/v2/portfolio/balance`) |
| Signature error with query params | Strip query parameters before signing (use `path.split('?')[0]`) |

## [​](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests\#next-steps)  Next Steps

Now you can make authenticated requests! Try these endpoints (relative to `BASE_URL`):

- `/portfolio/positions` \- Get your positions
- `/portfolio/orders` \- View your orders
- `/markets` \- Browse available markets

For more details, see the [Complete Order Lifecycle](https://docs.kalshi.com/getting_started/quick_start_create_order) guide or explore the [API Reference](https://docs.kalshi.com/api-reference).

[Quick Start: Market Data (No SDK)](https://docs.kalshi.com/getting_started/quick_start_market_data) [Quick Start: Create your first order (No SDK)](https://docs.kalshi.com/getting_started/quick_start_create_order)

Ctrl+I

[Powered byThis documentation is built and hosted on Mintlify, a developer documentation platform](https://www.mintlify.com/?utm_campaign=poweredBy&utm_medium=referral&utm_source=kalshi-b198743e)

Assistant

Responses are generated using AI and may contain mistakes.

[Skip to main content](https://docs.kalshi.com/getting_started/quick_start_create_order#content-area)

[API Documentation home page![light logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)![dark logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)](https://docs.kalshi.com/)

Search...

Ctrl KAsk AI

Search...

Navigation

Quick Start: Create your first order (No SDK)

[Welcome](https://docs.kalshi.com/welcome) [Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) [Concepts](https://docs.kalshi.com/getting_started/making_your_first_request) [REST](https://docs.kalshi.com/api-reference/historical/get-historical-cutoff-timestamps) [Websockets](https://docs.kalshi.com/websockets/websocket-connection) [FIX](https://docs.kalshi.com/fix) [SDKs](https://docs.kalshi.com/sdks/overview) [Changelog](https://docs.kalshi.com/changelog)

- [Quick Start: Market Data (No SDK)](https://docs.kalshi.com/getting_started/quick_start_market_data)

- [Quick Start: Authenticated Requests (No SDK)](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests)

- [Quick Start: Create your first order (No SDK)](https://docs.kalshi.com/getting_started/quick_start_create_order)

- [Quick Start: WebSockets (No SDK)](https://docs.kalshi.com/getting_started/quick_start_websockets)

On this page

- [Prerequisites](https://docs.kalshi.com/getting_started/quick_start_create_order#prerequisites)
- [Step 1: Find an Open Market](https://docs.kalshi.com/getting_started/quick_start_create_order#step-1-find-an-open-market)
- [Step 2: Place a Buy Order](https://docs.kalshi.com/getting_started/quick_start_create_order#step-2-place-a-buy-order)
- [Complete Example Script](https://docs.kalshi.com/getting_started/quick_start_create_order#complete-example-script)
- [Important Notes](https://docs.kalshi.com/getting_started/quick_start_create_order#important-notes)
- [Client Order ID](https://docs.kalshi.com/getting_started/quick_start_create_order#client-order-id)
- [Error Handling](https://docs.kalshi.com/getting_started/quick_start_create_order#error-handling)
- [Next Steps](https://docs.kalshi.com/getting_started/quick_start_create_order#next-steps)

# Quick Start: Create your first order (No SDK)

Learn how to find markets, place orders, check status, and cancel orders on Kalshi

This guide will walk you through the complete lifecycle of placing and managing orders on Kalshi.

## [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#prerequisites)  Prerequisites

Before you begin, you’ll need:

- A Kalshi account with API access configured
- Python with the `requests` and `cryptography` libraries installed
- Your authentication functions set up (see our [authentication guide](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests))

This guide assumes you have the authentication code from our authentication guide, including the `get()` function for making authenticated requests.

## [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#step-1-find-an-open-market)  Step 1: Find an Open Market

First, let’s find an open market to trade on.

```
# Get the first open market (no auth required for public market data)
response = requests.get('https://demo-api.kalshi.co/trade-api/v2/markets?limit=1&status=open')
market = response.json()['markets'][0]

print(f"Selected market: {market['ticker']}")
print(f"Title: {market['title']}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#step-2-place-a-buy-order)  Step 2: Place a Buy Order

Now let’s place an order to buy 1 YES contract for 1 cent (limit order). We’ll use a `client_order_id` to deduplicate orders - this allows you to identify duplicate orders before receiving the server-generated `order_id` in the response.

```
import uuid

def post(private_key, api_key_id, path, data, base_url=BASE_URL):
    """Make an authenticated POST request to the Kalshi API."""
    timestamp = str(int(datetime.datetime.now().timestamp() * 1000))
    signature = create_signature(private_key, timestamp, "POST", path)

    headers = {
        'KALSHI-ACCESS-KEY': api_key_id,
        'KALSHI-ACCESS-SIGNATURE': signature,
        'KALSHI-ACCESS-TIMESTAMP': timestamp,
        'Content-Type': 'application/json'
    }

    return requests.post(base_url + path, headers=headers, json=data)

# Place a buy order for 1 YES contract at 1 cent
order_data = {
    "ticker": market['ticker'],
    "action": "buy",
    "side": "yes",
    "count": 1,
    "type": "limit",
    "yes_price": 1,
    "client_order_id": str(uuid.uuid4())  # Unique ID for deduplication
}

response = post(private_key, API_KEY_ID, '/trade-api/v2/portfolio/orders', order_data)

if response.status_code == 201:
    order = response.json()['order']
    print(f"Order placed successfully!")
    print(f"Order ID: {order['order_id']}")
    print(f"Client Order ID: {order_data['client_order_id']}")
    print(f"Status: {order['status']}")
else:
    print(f"Error: {response.status_code} - {response.text}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#complete-example-script)  Complete Example Script

Here’s a complete script that creates your first order:

```
import requests
import uuid
# Assumes you have the authentication code from the prerequisites

# Add POST function to your existing auth code
def post(private_key, api_key_id, path, data, base_url=BASE_URL):
    """Make an authenticated POST request to the Kalshi API."""
    timestamp = str(int(datetime.datetime.now().timestamp() * 1000))
    signature = create_signature(private_key, timestamp, "POST", path)

    headers = {
        'KALSHI-ACCESS-KEY': api_key_id,
        'KALSHI-ACCESS-SIGNATURE': signature,
        'KALSHI-ACCESS-TIMESTAMP': timestamp,
        'Content-Type': 'application/json'
    }

    return requests.post(base_url + path, headers=headers, json=data)

# Step 1: Find an open market
print("Finding an open market...")
response = requests.get('https://demo-api.kalshi.co/trade-api/v2/markets?limit=1&status=open')
market = response.json()['markets'][0]
print(f"Selected: {market['ticker']} - {market['title']}")

# Step 2: Place a buy order
print("\nPlacing order...")
client_order_id = str(uuid.uuid4())
order_data = {
    "ticker": market['ticker'],
    "action": "buy",
    "side": "yes",
    "count": 1,
    "type": "limit",
    "yes_price": 1,
    "client_order_id": client_order_id
}

response = post(private_key, API_KEY_ID, '/trade-api/v2/portfolio/orders', order_data)

if response.status_code == 201:
    order = response.json()['order']
    print(f"Order placed successfully!")
    print(f"Order ID: {order['order_id']}")
    print(f"Client Order ID: {client_order_id}")
    print(f"Status: {order['status']}")
else:
    print(f"Error: {response.status_code} - {response.text}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#important-notes)  Important Notes

### [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#client-order-id)  Client Order ID

The `client_order_id` field is crucial for order deduplication:

- Generate a unique ID (like UUID4) for each order before submission
- If network issues occur, you can resubmit with the same `client_order_id`
- The API will reject duplicate submissions, preventing accidental double orders
- Store this ID locally to track orders before receiving the server’s `order_id`

### [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#error-handling)  Error Handling

Common errors and how to handle them:

- `401 Unauthorized`: Check your API keys and signature generation
- `400 Bad Request`: Verify your order parameters (price must be 1-99 cents)
- `409 Conflict`: Order with this `client_order_id` already exists
- `429 Too Many Requests`: You’ve hit the rate limit - slow down your requests

## [​](https://docs.kalshi.com/getting_started/quick_start_create_order\#next-steps)  Next Steps

Now that you’ve created your first order, you can:

- Check order status using the `/portfolio/orders/{order_id}` endpoint
- List all your orders with `/portfolio/orders`
- Amend your order price or quantity using PUT `/portfolio/orders/{order_id}`
- Cancel orders using DELETE `/portfolio/orders/{order_id}`
- Implement WebSocket connections for real-time updates
- Build automated trading strategies

For more information, check out:

- [API Reference Documentation](https://docs.kalshi.com/api-reference)
- [Kalshi Discord Community](https://discord.gg/kalshi)

[Quick Start: Authenticated Requests (No SDK)](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests) [Quick Start: WebSockets (No SDK)](https://docs.kalshi.com/getting_started/quick_start_websockets)

Ctrl+I

[Powered byThis documentation is built and hosted on Mintlify, a developer documentation platform](https://www.mintlify.com/?utm_campaign=poweredBy&utm_medium=referral&utm_source=kalshi-b198743e)

Assistant

Responses are generated using AI and may contain mistakes.

[Skip to main content](https://docs.kalshi.com/getting_started/quick_start_websockets#content-area)

[API Documentation home page![light logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)![dark logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)](https://docs.kalshi.com/)

Search...

Ctrl KAsk AI

Search...

Navigation

Quick Start: WebSockets (No SDK)

[Welcome](https://docs.kalshi.com/welcome) [Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) [Concepts](https://docs.kalshi.com/getting_started/making_your_first_request) [REST](https://docs.kalshi.com/api-reference/historical/get-historical-cutoff-timestamps) [Websockets](https://docs.kalshi.com/websockets/websocket-connection) [FIX](https://docs.kalshi.com/fix) [SDKs](https://docs.kalshi.com/sdks/overview) [Changelog](https://docs.kalshi.com/changelog)

- [Quick Start: Market Data (No SDK)](https://docs.kalshi.com/getting_started/quick_start_market_data)

- [Quick Start: Authenticated Requests (No SDK)](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests)

- [Quick Start: Create your first order (No SDK)](https://docs.kalshi.com/getting_started/quick_start_create_order)

- [Quick Start: WebSockets (No SDK)](https://docs.kalshi.com/getting_started/quick_start_websockets)

On this page

- [Overview](https://docs.kalshi.com/getting_started/quick_start_websockets#overview)
- [Connection URL](https://docs.kalshi.com/getting_started/quick_start_websockets#connection-url)
- [Authentication](https://docs.kalshi.com/getting_started/quick_start_websockets#authentication)
- [Required Headers](https://docs.kalshi.com/getting_started/quick_start_websockets#required-headers)
- [Signing the WebSocket Request](https://docs.kalshi.com/getting_started/quick_start_websockets#signing-the-websocket-request)
- [Establishing a Connection](https://docs.kalshi.com/getting_started/quick_start_websockets#establishing-a-connection)
- [Subscribing to Data](https://docs.kalshi.com/getting_started/quick_start_websockets#subscribing-to-data)
- [Processing Messages](https://docs.kalshi.com/getting_started/quick_start_websockets#processing-messages)
- [Connection Keep-Alive](https://docs.kalshi.com/getting_started/quick_start_websockets#connection-keep-alive)
- [Subscribing to Channels](https://docs.kalshi.com/getting_started/quick_start_websockets#subscribing-to-channels)
- [Subscribe to Ticker Updates](https://docs.kalshi.com/getting_started/quick_start_websockets#subscribe-to-ticker-updates)
- [Subscribe to Specific Markets](https://docs.kalshi.com/getting_started/quick_start_websockets#subscribe-to-specific-markets)
- [Connection Lifecycle](https://docs.kalshi.com/getting_started/quick_start_websockets#connection-lifecycle)
- [Error Handling](https://docs.kalshi.com/getting_started/quick_start_websockets#error-handling)
- [WebSocket Error Codes](https://docs.kalshi.com/getting_started/quick_start_websockets#websocket-error-codes)
- [Best Practices](https://docs.kalshi.com/getting_started/quick_start_websockets#best-practices)
- [Complete Example](https://docs.kalshi.com/getting_started/quick_start_websockets#complete-example)
- [Next Steps](https://docs.kalshi.com/getting_started/quick_start_websockets#next-steps)

# Quick Start: WebSockets (No SDK)

Learn how to establish and maintain a WebSocket connection to stream real-time market data

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#overview)  Overview

Kalshi’s WebSocket API provides real-time updates for:

- Order book changes
- Trade executions
- Market status updates
- Fill notifications

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#connection-url)  Connection URL

Connect to the WebSocket endpoint at:

```
wss://api.elections.kalshi.com/trade-api/ws/v2
```

For the demo environment, use:

```
wss://demo-api.kalshi.co/trade-api/ws/v2
```

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#authentication)  Authentication

WebSocket connections require authentication during the connection handshake.Once connected, channels fall into two groups:

- **Private channels (user-specific data):**`orderbook_delta`, `fill`, `market_positions`, `communications`, `order_group_updates`
- **Public market-data channels (no additional channel-level auth):**`ticker`, `trade`, `market_lifecycle_v2`, `multivariate_market_lifecycle`, `multivariate`

In other words, even channels that carry public market data still use the authenticated WebSocket session, but they do not impose additional per-channel authorization checks.

For detailed information about API key generation and request signing, see our [API Keys documentation](https://docs.kalshi.com/getting_started/api_keys).

### [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#required-headers)  Required Headers

When establishing the WebSocket connection, include these headers:

```
KALSHI-ACCESS-KEY: your_api_key_id
KALSHI-ACCESS-SIGNATURE: request_signature
KALSHI-ACCESS-TIMESTAMP: unix_timestamp_in_milliseconds
```

### [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#signing-the-websocket-request)  Signing the WebSocket Request

The signature for WebSocket connections follows the same pattern as REST API requests:

1. **Create the message to sign:**






















```
timestamp + "GET" + "/trade-api/ws/v2"
```

2. **Generate the signature** using your private key (see [API Keys documentation](https://docs.kalshi.com/getting_started/api_keys))
3. **Include the headers** when opening the WebSocket connection

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#establishing-a-connection)  Establishing a Connection

To connect to the WebSocket API, you need to:

1. Generate authentication headers (same as REST API)
2. Create a WebSocket connection with those headers
3. Handle the connection lifecycle

Here’s how to establish an authenticated connection:

```
import websockets
import asyncio

# WebSocket URL
ws_url = "wss://demo-api.kalshi.co/trade-api/ws/v2"  # Demo environment

# Generate authentication headers (see API Keys documentation)
auth_headers = {
    "KALSHI-ACCESS-KEY": "your_api_key_id",
    "KALSHI-ACCESS-SIGNATURE": "generated_signature",
    "KALSHI-ACCESS-TIMESTAMP": "timestamp_in_milliseconds"
}

# Connect with authentication
async def connect():
    async with websockets.connect(ws_url, additional_headers=auth_headers) as websocket:
        print("Connected to Kalshi WebSocket")

        # Connection is now established
        # You can start sending and receiving messages

        # Listen for messages
        async for message in websocket:
            print(f"Received: {message}")

# Run the connection
asyncio.run(connect())
```

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#subscribing-to-data)  Subscribing to Data

Once connected, subscribe to channels by sending a subscription command:

```
import json

async def subscribe_to_ticker(websocket):
    """Subscribe to ticker updates"""
    subscription = {
        "id": 1,
        "cmd": "subscribe",
        "params": {
            "channels": ["ticker"]
        }
    }
    await websocket.send(json.dumps(subscription))

async def subscribe_to_orderbook(websocket, market_tickers):
    """Subscribe to orderbook updates for specific markets"""
    subscription = {
        "id": 2,
        "cmd": "subscribe",
        "params": {
            "channels": ["orderbook_delta"],
            "market_tickers": market_tickers
        }
    }
    await websocket.send(json.dumps(subscription))
```

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#processing-messages)  Processing Messages

Handle incoming messages based on their type:

```
async def process_message(message):
    """Process incoming WebSocket messages"""
    data = json.loads(message)
    msg_type = data.get("type")

    if msg_type == "ticker":
        # Handle ticker update
        market = data["msg"]["market_ticker"]
        bid = data["msg"]["yes_bid_dollars"]
        ask = data["msg"]["yes_ask_dollars"]
        print(f"{market}: Yes Bid ${bid}, Yes Ask ${ask}")

    elif msg_type == "orderbook_snapshot":
        # Handle full orderbook state
        print(f"Orderbook snapshot for {data['msg']['market_ticker']}")

    elif msg_type == "orderbook_delta":
        # Handle orderbook changes
        print(f"Orderbook update for {data['msg']['market_ticker']}")
        # Note: client_order_id field is optional - present only when you caused this change
        if 'client_order_id' in data['msg']:
            print(f"  Your order {data['msg']['client_order_id']} caused this change")

    elif msg_type == "error":
        error_code = data.get("msg", {}).get("code")
        error_msg = data.get("msg", {}).get("msg")
        print(f"Error {error_code}: {error_msg}")
```

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#connection-keep-alive)  Connection Keep-Alive

The Python `websockets` library automatically handles WebSocket ping/pong frames to keep connections alive. No manual heartbeat handling is required. Learn more about [automatic keepalive in the websockets documentation](https://websockets.readthedocs.io/en/stable/topics/design.html#keepalive).Other WebSocket libraries may require manual ping/pong implementation.

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#subscribing-to-channels)  Subscribing to Channels

Once connected, subscribe to specific data channels:

### [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#subscribe-to-ticker-updates)  Subscribe to Ticker Updates

To receive real-time ticker updates for all markets:

```
async def subscribe_to_tickers(self):
    """Subscribe to ticker updates for all markets"""
    subscription_message = {
        "id": self.message_id,
        "cmd": "subscribe",
        "params": {
            "channels": ["ticker"]
        }
    }
    await self.ws.send(json.dumps(subscription_message))
    self.message_id += 1
```

### [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#subscribe-to-specific-markets)  Subscribe to Specific Markets

To subscribe to orderbook or trade updates for specific markets:

```
async def subscribe_to_markets(self, channels, market_tickers):
    """Subscribe to specific channels and markets"""
    subscription_message = {
        "id": self.message_id,
        "cmd": "subscribe",
        "params": {
            "channels": channels,
            "market_tickers": market_tickers
        }
    }
    await self.ws.send(json.dumps(subscription_message))
    self.message_id += 1

# Example usage:
# Subscribe to orderbook updates
await subscribe_to_markets(["orderbook_delta"], ["KXFUT24-LSV", "KXHARRIS24-LSV"])

# Subscribe to trade feed
await subscribe_to_markets(["trade"], ["KXFUT24-LSV"])
```

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#connection-lifecycle)  Connection Lifecycle

1. **Initial Connection**: Establish WebSocket with authentication headers
2. **Subscribe**: Send subscription commands for desired channels
3. **Receive Updates**: Process incoming messages based on their type
4. **Handle Disconnects**: Implement reconnection logic with exponential backoff

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#error-handling)  Error Handling

The server sends error messages in this format:

```
{
  "id": 123,
  "type": "error",
  "msg": {
    "code": 2,
    "msg": "Params required"
  }
}
```

### [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#websocket-error-codes)  WebSocket Error Codes

| Code | Error | Description |
| --- | --- | --- |
| 1 | Unable to process message | General processing error |
| 2 | Params required | Missing params object in command |
| 3 | Channels required | Missing channels array in subscribe |
| 4 | Subscription IDs required | Missing sids in unsubscribe |
| 5 | Unknown command | Invalid command name |
| 6 | Already subscribed | Duplicate subscription attempt |
| 7 | Unknown subscription ID | Subscription ID not found |
| 8 | Unknown channel name | Invalid channel in subscribe |
| 9 | Authentication required | Private channel without auth |
| 10 | Channel error | Channel-specific error |
| 11 | Invalid parameter | Malformed parameter value |
| 12 | Exactly one subscription ID is required | For update\_subscription |
| 13 | Unsupported action | Invalid action for update\_subscription |
| 14 | Market Ticker required | Missing market specification (market\_ticker or market\_id) |
| 15 | Action required | Missing action in update\_subscription |
| 16 | Market not found | Invalid market\_ticker or market\_id |
| 17 | Internal error | Server-side processing error |
| 18 | Command timeout | Server timed out while processing command |
| 19 | shard\_factor must be > 0 | Invalid shard\_factor |
| 20 | shard\_factor is required when shard\_key is set | Missing shard\_factor when shard\_key is set |
| 21 | shard\_key must be >= 0 and < shard\_factor | Invalid shard\_key |
| 22 | shard\_factor must be <= 100 | shard\_factor too large |

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#best-practices)  Best Practices

## Connection Management

- Implement automatic reconnection with exponential backoff
- Handle network interruptions gracefully
- Use the websockets library’s built-in keepalive

## Data Handling

- Process messages asynchronously to avoid blocking
- Implement proper error handling for malformed messages
- Cache initial orderbook state before applying updates

## Security

- Never expose your private key in client-side code
- Rotate API keys regularly
- Use secure key storage practices

## Performance

- Subscribe only to markets you need
- Implement message buffering for high-frequency updates
- Consider using connection pooling for multiple subscriptions

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#complete-example)  Complete Example

Here’s a complete, runnable example that connects to the WebSocket API and subscribes to orderbook updates:

```
import asyncio
import base64
import json
import time
import websockets
from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import padding

# Configuration
KEY_ID = "your_api_key_id"
PRIVATE_KEY_PATH = "path/to/private_key.pem"
MARKET_TICKER = "KXHARRIS24-LSV"  # Replace with any open market
WS_URL = "wss://demo-api.kalshi.co/trade-api/ws/v2"

def sign_pss_text(private_key, text: str) -> str:
    """Sign message using RSA-PSS"""
    message = text.encode('utf-8')
    signature = private_key.sign(
        message,
        padding.PSS(
            mgf=padding.MGF1(hashes.SHA256()),
            salt_length=padding.PSS.DIGEST_LENGTH
        ),
        hashes.SHA256()
    )
    return base64.b64encode(signature).decode('utf-8')

def create_headers(private_key, method: str, path: str) -> dict:
    """Create authentication headers"""
    timestamp = str(int(time.time() * 1000))
    msg_string = timestamp + method + path.split('?')[0]
    signature = sign_pss_text(private_key, msg_string)

    return {
        "Content-Type": "application/json",
        "KALSHI-ACCESS-KEY": KEY_ID,
        "KALSHI-ACCESS-SIGNATURE": signature,
        "KALSHI-ACCESS-TIMESTAMP": timestamp,
    }

async def orderbook_websocket():
    """Connect to WebSocket and subscribe to orderbook"""
    # Load private key
    with open(PRIVATE_KEY_PATH, 'rb') as f:
        private_key = serialization.load_pem_private_key(
            f.read(),
            password=None
        )

    # Create WebSocket headers
    ws_headers = create_headers(private_key, "GET", "/trade-api/ws/v2")

    async with websockets.connect(WS_URL, additional_headers=ws_headers) as websocket:
        print(f"Connected! Subscribing to orderbook for {MARKET_TICKER}")

        # Subscribe to orderbook
        subscribe_msg = {
            "id": 1,
            "cmd": "subscribe",
            "params": {
                "channels": ["orderbook_delta"],
                "market_ticker": MARKET_TICKER
            }
        }
        await websocket.send(json.dumps(subscribe_msg))

        # Process messages
        async for message in websocket:
            data = json.loads(message)
            msg_type = data.get("type")

            if msg_type == "subscribed":
                print(f"Subscribed: {data}")

            elif msg_type == "orderbook_snapshot":
                print(f"Orderbook snapshot: {data}")

            elif msg_type == "orderbook_delta":
                # The client_order_id field is optional - only present when you caused the change
                if 'client_order_id' in data.get('msg', {}):
                    print(f"Orderbook update (your order {data['msg']['client_order_id']}): {data}")
                else:
                    print(f"Orderbook update: {data}")

            elif msg_type == "error":
                print(f"Error: {data}")

# Run the example
if __name__ == "__main__":
    asyncio.run(orderbook_websocket())
```

This example:

- Establishes an authenticated WebSocket connection
- Subscribes to orderbook updates for the specified market
- Processes both the initial snapshot and incremental updates
- Displays orderbook changes in real-time

To run this example:

1. Replace `KEY_ID` with your API key ID
2. Replace `PRIVATE_KEY_PATH` with the path to your private key file
3. Replace `MARKET_TICKER` with any open market ticker
4. Run with Python 3.7+

## [​](https://docs.kalshi.com/getting_started/quick_start_websockets\#next-steps)  Next Steps

- Review the [WebSocket API Reference](https://docs.kalshi.com/websockets) for detailed message specifications
- Explore [Market Data Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) for REST API integration
- Check out our [Demo Environment](https://docs.kalshi.com/getting_started/demo_env) for testing

[Quick Start: Create your first order (No SDK)](https://docs.kalshi.com/getting_started/quick_start_create_order)

Ctrl+I

[Powered byThis documentation is built and hosted on Mintlify, a developer documentation platform](https://www.mintlify.com/?utm_campaign=poweredBy&utm_medium=referral&utm_source=kalshi-b198743e)

Assistant

Responses are generated using AI and may contain mistakes.

[Skip to main content](https://docs.kalshi.com/fix#content-area)

[API Documentation home page![light logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)![dark logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)](https://docs.kalshi.com/)

Search...

Ctrl KAsk AI

Search...

Navigation

FIX

FIX API Overview

[Welcome](https://docs.kalshi.com/welcome) [Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) [Concepts](https://docs.kalshi.com/getting_started/making_your_first_request) [REST](https://docs.kalshi.com/api-reference/historical/get-historical-cutoff-timestamps) [Websockets](https://docs.kalshi.com/websockets/websocket-connection) [FIX](https://docs.kalshi.com/fix) [SDKs](https://docs.kalshi.com/sdks/overview) [Changelog](https://docs.kalshi.com/changelog)

##### FIX

- [FIX API Overview](https://docs.kalshi.com/fix)
- [Connectivity](https://docs.kalshi.com/fix/connectivity)
- [Session Management](https://docs.kalshi.com/fix/session-management)
- [Order Entry Messages](https://docs.kalshi.com/fix/order-entry)
- [Order Group Messages](https://docs.kalshi.com/fix/order-groups)
- [RFQ Messages](https://docs.kalshi.com/fix/rfq-messages)
- [Drop Copy Session](https://docs.kalshi.com/fix/drop-copy)
- [Market Settlement](https://docs.kalshi.com/fix/market-settlement)
- [Error Handling](https://docs.kalshi.com/fix/error-handling)
- [Subpenny Pricing](https://docs.kalshi.com/fix/subpenny-pricing)

On this page

- [Kalshi FIX API Specifications](https://docs.kalshi.com/fix#kalshi-fix-api-specifications)
- [Introduction](https://docs.kalshi.com/fix#introduction)
- [FIX Dictionary Download](https://docs.kalshi.com/fix#fix-dictionary-download)
- [Key Features](https://docs.kalshi.com/fix#key-features)
- [Getting Started](https://docs.kalshi.com/fix#getting-started)
- [Support](https://docs.kalshi.com/fix#support)

FIX

# FIX API Overview

Financial Information eXchange (FIX) protocol implementation for Kalshi

# [​](https://docs.kalshi.com/fix\#kalshi-fix-api-specifications)  Kalshi FIX API Specifications

**Version**: 1.0.16
**Last Updated**: 2025-11-30

## [​](https://docs.kalshi.com/fix\#introduction)  Introduction

FIX (Financial Information eXchange) is a standard protocol that can be used to enter orders, submit cancel requests, and receive fills. Kalshi’s implementation follows the standards as closely as possible, with any divergences highlighted in this documentation.Please contact [institutional@kalshi.com](mailto:institutional@kalshi.com) to inquire about FIX access.

## [​](https://docs.kalshi.com/fix\#fix-dictionary-download)  FIX Dictionary Download

Need a machine-readable view of the Kalshi-specific FIX tags and messages? Download the XML dictionary and import it into your FIX tooling of choice:

- [Kalshi FIX Dictionary v0.1 (XML)](https://kalshi-public-docs.s3.us-east-1.amazonaws.com/fix/kalshi-fix-dictionary.xml)

## [​](https://docs.kalshi.com/fix\#key-features)  Key Features

[**Session Management** \\
\\
Logon, logout, heartbeat, and session control messages](https://docs.kalshi.com/fix/session-management)

[**Order Entry** \\
\\
Submit, modify, and cancel orders through standard FIX messages](https://docs.kalshi.com/fix/order-entry)

[**Market Settlement** \\
\\
Market settlement and payout updates](https://docs.kalshi.com/fix/market-settlement)

[**RFQ Support** \\
\\
Request for Quote functionality for market makers](https://docs.kalshi.com/fix/rfq-messages)

[**Drop Copy** \\
\\
Separate session for order event recovery](https://docs.kalshi.com/fix/drop-copy)

[**Order Groups** \\
\\
Automatic order cancellation with contracts limits](https://docs.kalshi.com/fix/order-groups)

## [​](https://docs.kalshi.com/fix\#getting-started)  Getting Started

1

[Navigate to header](https://docs.kalshi.com/fix#)

Generate RSA Keys

Create a 2048 bit RSA PKCS#8 key pair for authentication

2

[Navigate to header](https://docs.kalshi.com/fix#)

Create API Key

Upload your public key to the Kalshi platform to receive your FIX API key

3

[Navigate to header](https://docs.kalshi.com/fix#)

Configure Connection

Set up your FIX client with the appropriate endpoints and credentials

4

[Navigate to header](https://docs.kalshi.com/fix#)

Start Trading

Send a Logon message and begin submitting orders

## [​](https://docs.kalshi.com/fix\#support)  Support

For technical support or questions about the FIX API, please contact the Kalshi trading support team.

[Connectivity](https://docs.kalshi.com/fix/connectivity)

Ctrl+I

[Powered byThis documentation is built and hosted on Mintlify, a developer documentation platform](https://www.mintlify.com/?utm_campaign=poweredBy&utm_medium=referral&utm_source=kalshi-b198743e)

Assistant

Responses are generated using AI and may contain mistakes.

[Skip to main content](https://docs.kalshi.com/fix/connectivity#content-area)

[API Documentation home page![light logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)![dark logo](https://mintcdn.com/kalshi-b198743e/Q18sjVFWZz0Uu7ZQ/logo.svg?fit=max&auto=format&n=Q18sjVFWZz0Uu7ZQ&q=85&s=7364419fd9a0b4018762d0027572202b)](https://docs.kalshi.com/)

Search...

Ctrl KAsk AI

Search...

Navigation

FIX

Connectivity

[Welcome](https://docs.kalshi.com/welcome) [Quick Start](https://docs.kalshi.com/getting_started/quick_start_market_data) [Concepts](https://docs.kalshi.com/getting_started/making_your_first_request) [REST](https://docs.kalshi.com/api-reference/historical/get-historical-cutoff-timestamps) [Websockets](https://docs.kalshi.com/websockets/websocket-connection) [FIX](https://docs.kalshi.com/fix) [SDKs](https://docs.kalshi.com/sdks/overview) [Changelog](https://docs.kalshi.com/changelog)

##### FIX

- [FIX API Overview](https://docs.kalshi.com/fix)
- [Connectivity](https://docs.kalshi.com/fix/connectivity)
- [Session Management](https://docs.kalshi.com/fix/session-management)
- [Order Entry Messages](https://docs.kalshi.com/fix/order-entry)
- [Order Group Messages](https://docs.kalshi.com/fix/order-groups)
- [RFQ Messages](https://docs.kalshi.com/fix/rfq-messages)
- [Drop Copy Session](https://docs.kalshi.com/fix/drop-copy)
- [Market Settlement](https://docs.kalshi.com/fix/market-settlement)
- [Error Handling](https://docs.kalshi.com/fix/error-handling)
- [Subpenny Pricing](https://docs.kalshi.com/fix/subpenny-pricing)

On this page

- [FIX API Connectivity](https://docs.kalshi.com/fix/connectivity#fix-api-connectivity)
- [Endpoints](https://docs.kalshi.com/fix/connectivity#endpoints)
- [Rate Limits](https://docs.kalshi.com/fix/connectivity#rate-limits)
- [Order Entry Session](https://docs.kalshi.com/fix/connectivity#order-entry-session)
- [TCP SSL Configuration](https://docs.kalshi.com/fix/connectivity#tcp-ssl-configuration)
- [SSL/TLS Requirements](https://docs.kalshi.com/fix/connectivity#ssl%2Ftls-requirements)
- [Message Retransmission](https://docs.kalshi.com/fix/connectivity#message-retransmission)
- [Supported Endpoints](https://docs.kalshi.com/fix/connectivity#supported-endpoints)
- [Unsupported Message Types](https://docs.kalshi.com/fix/connectivity#unsupported-message-types)
- [Alternative Recovery](https://docs.kalshi.com/fix/connectivity#alternative-recovery)
- [Session Configuration](https://docs.kalshi.com/fix/connectivity#session-configuration)
- [Required Settings](https://docs.kalshi.com/fix/connectivity#required-settings)
- [Session Identification](https://docs.kalshi.com/fix/connectivity#session-identification)
- [Best Practices](https://docs.kalshi.com/fix/connectivity#best-practices)
- [Troubleshooting](https://docs.kalshi.com/fix/connectivity#troubleshooting)
- [Common Connection Issues](https://docs.kalshi.com/fix/connectivity#common-connection-issues)

FIX

# Connectivity

Connection setup and endpoints for Kalshi FIX API

# [​](https://docs.kalshi.com/fix/connectivity\#fix-api-connectivity)  FIX API Connectivity

## [​](https://docs.kalshi.com/fix/connectivity\#endpoints)  Endpoints

Before logging onto a FIX session, clients must establish a secure connection to the FIX gateway.

- Production

- Demo


**Host:**`fix.elections.kalshi.com`

| Purpose | Port | TargetCompID |
| --- | --- | --- |
| Order Entry (without retransmission) | 8228 | KalshiNR |
| Order Entry (with retransmission) | 8230 | KalshiRT |
| Drop Copy | 8229 | KalshiDC |
| Post Trade | 8231 | KalshiPT |
| RFQ | 8232 | KalshiRFQ |

**Host:**`fix.demo.kalshi.co`

| Purpose | Port | TargetCompID |
| --- | --- | --- |
| Order Entry (without retransmission) | 8228 | KalshiNR |
| Order Entry (with retransmission) | 8230 | KalshiRT |
| Drop Copy | 8229 | KalshiDC |
| Post Trade | 8231 | KalshiPT |
| RFQ | 8232 | KalshiRFQ |

Sessions are potentially dropped during trading closed hours for maintenance. For now, this is on Thursdays from 3 AM to 5 AM ET. All users are required to restart their sessions during this time and reset sequence numbers to 0.

## [​](https://docs.kalshi.com/fix/connectivity\#rate-limits)  Rate Limits

### [​](https://docs.kalshi.com/fix/connectivity\#order-entry-session)  Order Entry Session

- **Limit**: Your account-level rate limits are applicable
- **Scope**: Application messages only (from client to server)
- **Excluded**: Session layer messages

Session layer messages excluded from rate limits:

- Logout (35=5)
- Heartbeat (35=0)
- TestRequest (35=1)

Logon (35=A) is rate-limited.

## [​](https://docs.kalshi.com/fix/connectivity\#tcp-ssl-configuration)  TCP SSL Configuration

### [​](https://docs.kalshi.com/fix/connectivity\#ssl/tls-requirements)  SSL/TLS Requirements

**You must use TLS/SSL (not plain TCP) to connect to the FIX gateway.** Plain TCP connections will fail because the gateway requires a TLS handshake.If your FIX implementation does not support native TLS connections, set up a local proxy such as stunnel to establish a secure connection.

Kalshi will provide the certificate for pinning on the initiator side when providing your API key.

## [​](https://docs.kalshi.com/fix/connectivity\#message-retransmission)  Message Retransmission

### [​](https://docs.kalshi.com/fix/connectivity\#supported-endpoints)  Supported Endpoints

Message retransmission is currently only supported on:

- Order Entry with retransmission (KalshiRT)
- RFQ session (KalshiRFQ)

### [​](https://docs.kalshi.com/fix/connectivity\#unsupported-message-types)  Unsupported Message Types

For endpoints without retransmission support:

- ResendRequest (35=2) - Not supported
- SequenceReset (35=4) - Not supported

For sessions without retransmission support, `ResetSeqNumFlag&lt;141&gt;` in the Logon message must always be `true` or the Logon will be rejected.

### [​](https://docs.kalshi.com/fix/connectivity\#alternative-recovery)  Alternative Recovery

The drop copy session endpoint provides an alternative way for clients to query for missed execution reports without using the retransmission protocol.

## [​](https://docs.kalshi.com/fix/connectivity\#session-configuration)  Session Configuration

### [​](https://docs.kalshi.com/fix/connectivity\#required-settings)  Required Settings

- **Session Profile**: FIXT.1.1 (required for Application Version Independence)
- **Application Version**: FIX50SP2 (FIX 5.0 SP2)
- **SenderCompID**: Your FIX API Key (UUID format)
- **TargetCompID**: See endpoints table above

### [​](https://docs.kalshi.com/fix/connectivity\#session-identification)  Session Identification

- Session identification uses: `SessionID = TargetCompID + SenderCompID`
- Only one FIX connection is allowed per FIX API Key

Each API key can only be used for a single connection at a time. If you need to establish multiple concurrent connections (e.g., for both order entry and drop copy), you must create separate API keys for each connection.

## [​](https://docs.kalshi.com/fix/connectivity\#best-practices)  Best Practices

1. **Connection Management**   - Implement automatic reconnection logic for the daily maintenance window
   - Monitor heartbeat intervals (default 30 seconds)
   - Handle connection drops gracefully
2. **Sequence Number Management**   - Reset sequence numbers to 0 after daily maintenance
   - For non-retransmission endpoints, always use `ResetSeqNumFlag=Y`
3. **Security**   - Store private keys securely
   - Never share private keys, even with Kalshi employees
   - Use certificate pinning when provided

## [​](https://docs.kalshi.com/fix/connectivity\#troubleshooting)  Troubleshooting

### [​](https://docs.kalshi.com/fix/connectivity\#common-connection-issues)  Common Connection Issues

SSL/TLS Connection Failed

- **Verify you are using TLS, not plain TCP** \- this is the most common issue
- Check your FIX library settings to ensure TLS/SSL mode is enabled
- Verify certificate configuration
- Check if stunnel or similar proxy is needed if your library doesn’t support native TLS

Logon Rejected

- Verify SenderCompID matches your FIX API key
- Check TargetCompID matches the port number
- Ensure ResetSeqNumFlag is set correctly for non-retransmission endpoints
- Verify signature generation uses the exact SendingTime from field 52

[FIX API Overview](https://docs.kalshi.com/fix) [Session Management](https://docs.kalshi.com/fix/session-management)

Ctrl+I

[Powered byThis documentation is built and hosted on Mintlify, a developer documentation platform](https://www.mintlify.com/?utm_campaign=poweredBy&utm_medium=referral&utm_source=kalshi-b198743e)

Assistant

Responses are generated using AI and may contain mistakes.

FIX
Order Entry Messages
Submit, modify, and cancel orders through FIX messages
​
Order Entry Messages

​
Overview

Kalshi treats all orders as bids or asks for Yes contracts. Selling Yes is equivalent to buying No contracts.
​
New Order Single (35=D)

Used to submit a new order to the Exchange.
Tag	Name	Type	Required	Description
11	ClOrderID	String	Y	Client order identifier for idempotency. Must not match any open orders.

UUID format is preferred.

Validation:
• Maximum 64 characters
• Only alphanumeric, ”+”, ”=”, ”_”, ”-”, and ”:” characters allowed
• Pattern: ^[\-\w:+=/]*$
18	ExecInst	Char	N	Execution instruction flags.

Supported values:
6 = Post Only
38	OrderQty	Decimal	Y	Quantity of contracts to trade. Fractional quantities are supported.
40	OrdType	Char	Y	Order type.

Supported values:
2 = Limit
44	Price	Integer	Y	Price per contract in cents (1-99).
54	Side	Char	Y	Side of the order.

Supported values:
1 = Buy (Yes contracts)
2 = Sell (No contracts)
55	Symbol	String	Y	Market ticker (e.g., “EURUSD-23JUN2618-B1.087”).
59	TimeInForce	Char	N	Specifies how long the order remains in effect.

Supported values:
0 = Day (expires at 11:59:59.999pm ET)
1 = Good Till Cancel (GTC)
3 = Immediate Or Cancel (IOC)
4 = Fill Or Kill (FOK)
6 = Good Till Date (GTD)

Note: Day orders expire at 11:59:59.999pm ET. For GTD orders, any date in the past is treated as Immediate Or Cancel.
126	ExpireTime	UTCTimestamp	C	Required when TimeInForce=GTD. Specifies when the GTD order expires.
448	PartyID	UUID	N	Only applicable for FCM entities . Sub-account identifier.
452	PartyRole	Integer	N	Only applicable for FCM entities . Party role.

Supported values:
24 = Customer Account
453	NoPartyIDs	Integer	N	Only applicable for FCM entities . Number of parties. Currently, only 1 is supported.
79	AllocAccount	Integer	N	Subaccount number (0-32). Alternative to NoPartyIDs for specifying a subaccount.
526	SecondaryClOrdID	UUID	N	Order group identifier. Please refer to the Order Groups tab for more information.
2964	SelfTradePreventionType	Char	N	Self-trade prevention mode. If unset, defaults to Taker At Cross.

Supported values:
1 = Taker At Cross
2 = Maker
21006	CancelOrderOnPause	Boolean	N	If this flag is set to true, the order will be canceled if the order is open and trading on the exchange is paused for any reason.
21009	MaxExecutionCost	Decimal	N	Optional value representing max execution cost for an order in dollars. Order is canceled if unable to fill OrdQty for the given cost.

Example New Order
8=FIXT.1.1|9=200|35=D|34=5|52=20230809-12:34:56.789|49=your-api-key|56=KalshiNR|
11=550e8400-e29b-41d4-a716-446655440000|38=10|40=2|54=1|55=HIGHNY-23DEC31|44=75|
59=1|10=123|
​
Order Cancel/Replace Request (35=G)

Used to modify an existing order without canceling it.
​
Supported Modifications

OrderQty: Increases or decreases the quantity of your order, note that increasing the quantity for the same point means forfeiting your queue position
Price: Changes the limit price of your order
Tag	Name	Type	Required	Description
11	ClOrderID	String	Y	Unique modification request identifier. Must not match any existing ClOrderID.

UUID format is preferred.

Validation:
• Maximum 64 characters
• Only alphanumeric, ”+”, ”=”, ”_”, ”-”, and ”:” characters allowed
• Pattern: ^[\-\w:+=/]*$
37	OrderID	String	N	Order identifier provided by Kalshi Exchange.
38	OrderQty	Decimal	Y	New total quantity for the order. Fractional quantities are supported.

Note: If OrderQty equals filled quantity, the order will be canceled. If less than filled quantity, the request will be rejected.
40	OrdType	Char	Y	Order type.

Supported values:
2 = Limit
41	OrigClOrdID	String	Y	ClOrderID of the order to modify.
44	Price	Integer	N	New price per contract in cents (1-99). Required if changing price.
54	Side	Char	Y	Must match the original order side.
55	Symbol	String	Y	Must match the original order symbol.
448	PartyID	UUID	N	Only applicable for FCM entities . Sub-account identifier. Must match the original order field value.
452	PartyRole	Integer	N	Only applicable for FCM entities . Party role. Must match the original order field value.

Supported values:
24 = Customer Account
453	NoPartyIDs	Integer	N	Only applicable for FCM entities . Number of parties. Must match the original order field value. Currently, only 1 is supported.
79	AllocAccount	Integer	N	Subaccount number (0-32). Must match the original order. Alternative to NoPartyIDs for specifying a subaccount.
​
Order Cancel Request (35=F)

Cancel all remaining quantity of an existing order.
Tag	Name	Type	Required	Description
11	ClOrderID	String	Y	Unique cancel request identifier. Must not match any existing ClOrderID.

UUID format is preferred.

Validation:
• Maximum 64 characters
• Only alphanumeric, ”+”, ”=”, ”_”, ”-”, and ”:” characters allowed
• Pattern: ^[\-\w:+=/]*$
37	OrderID	String	N	Order identifier provided by Kalshi Exchange.
41	OrigClOrdID	String	Y	ClOrderID of the order to cancel.
54	Side	Char	Y	Must match the original order side.
55	Symbol	String	Y	Must match the original order symbol.
448	PartyID	UUID	N	Only applicable for FCM entities . Sub-account identifier. Must match the original order field value.
452	PartyRole	Integer	N	Only applicable for FCM entities . Party role. Must match the original order field value.

Supported values:
24 = Customer Account
453	NoPartyIDs	Integer	N	Only applicable for FCM entities . Number of parties. Must match the original order field value. Currently, only 1 is supported.
79	AllocAccount	Integer	N	Subaccount number (0-32). Must match the original order. Alternative to NoPartyIDs for specifying a subaccount.
​
Execution Report (35=8)

This message is sent by Kalshi Exchange back to clients to reflect changes to an order’s state (accepted, replaced, partially filled, filled or canceled).
Tag	Name	Type	Required	Description
6	AvgPx	Decimal	Y	Calculated average price of all fills on this order.
11	ClOrderID	String	Y	ClOrderID provided by the initiator on the last message that made any change to the order.
14	CumQty	Decimal	Y	Total quantity filled in this order so far.
17	ExecID	String	Y	Unique sequenced identifier for this report message.

Format: integer;integer pattern (e.g., “4;4”, “4;7”, “7;3”)

Monotonically increasing at the Exchange level. Returns “-1;-1” for PENDING execution reports.
31	LastPx	Integer	C	Price of this (last) fill in cents. Present only for trade execution reports.
32	LastQty	Decimal	C	Quantity bought or sold in this fill. Present only for trade execution reports.
37	OrderID	String	Y	Unique identifier for the order in the Kalshi Exchange. Use this ID when referencing the order for support.
38	OrderQty	Decimal	Y	Total quantity currently in the order. OrderQty = CumQty + LeavesQty.
39	OrdStatus	Char	Y	Current status of the order after this event. See Order Status values below.
41	OrigClOrdID	String	C	ClOrderID of the previous non-rejected order state. Present for Replaced/Canceled orders.
44	Price	Integer	C	Price per contract in cents.
54	Side	Char	Y	Side of the original order.

Values:
1 = Buy (Yes contracts)
2 = Sell (No contracts)
55	Symbol	String	Y	Market ticker for the order.
58	Text	String	N	Human-readable description of the execution report result.
60	TransactTime	UTCTimestamp	Y	Timestamp for the event that triggered this execution report.
103	OrdRejReason	Integer	C	Rejection reason. Present only when ExecType = Rejected. See Order Rejection Reasons below.
126	ExpireTime	UTCTimestamp	C	Order expiration timestamp. Returns 11:59pm ET for Day orders.
150	ExecType	Char	Y	Reason why this execution report was sent. See Execution Types below.
151	LeavesQty	Decimal	Y	Remaining quantity open for further execution on this order.
448	PartyID	UUID	N	Only applicable for FCM entities . Sub-account identifier.
452	PartyRole	Integer	N	Only applicable for FCM entities . Party role.

Supported values:
24 = Customer Account
453	NoPartyIDs	Integer	N	Only applicable for FCM entities . Number of parties. Currently, only 1 is supported.
79	AllocAccount	Integer	C	Subaccount number (0-32). Present if the order was placed for a subaccount.
​
Order Status (39)

New<0>: Active order, no fills
Partially Filled<1>: Some quantity filled
Filled<2>: Completely filled
Canceled<4>: Canceled (may have partial fills)
Pending Cancel<6>: Cancel pending
Rejected<8>: Order rejected
Pending New<A>: Order pending acceptance
Expired<C>: Time in force expired
Pending Replace<E>: Modification pending
By default, expiry-style system cancellations are reported as Canceled<4>.
If Logon tag 21012 (UseExpiredOrdStatus)=Y, expiry-style system cancellations (CloseCancel and OrderExpiryCancel) are reported as Expired<C>.
​
Order Rejection Reasons (103)

Unknown symbol<1>
Exchange closed<2>
Order exceeds limit<3>
Too late to enter<4>
Duplicate order<6>
Unsupported order characteristic<11>
Unknown account<15>
Other<99>
​
Execution Types (150)

New<0>: Order accepted
Trade<F>: Order filled (partial or complete)
Canceled<4>: Order canceled
Replaced<5>: Order modified
Rejected<8>: Order rejected
Expired<C>: Order expired
Pending New<A>: Order pending acceptance
Pending Cancel<6>: Cancel pending
Pending Replace<E>: Modification pending
With default settings, expiry-style system cancellations emit Canceled<4>.
If Logon tag 21012 (UseExpiredOrdStatus)=Y, expiry-style system cancellations (CloseCancel and OrderExpiryCancel) emit Expired<C>.
​
Text Field Values (58)

Common values for the Text field in Execution Reports:
EXCHANGE_UNAVAILABLE - Catch-all for Exchange outage or unmapped errors, maps to OrdRejReason “Other”
MARKET_ALREADY_CLOSED - maps to OrdRejReason “Exchange closed”
MARKET_INACTIVE - maps to OrdRejReason “Exchange closed”
MARKET_NOT_FOUND - maps to OrdRejReason “Unknown symbol”
SELF_CROSS_ATTEMPT - maps to ExecutionType “Canceled”
SELF_CROSS_ATTEMPT_PARTIALLY_FILLED - maps to ExecutionType “Canceled”
ORDER_ALREADY_EXISTS - maps to OrdRejReason “Duplicate order”
EXCEEDED_ORDER_GROUP_RISK_LIMIT - maps to OrdRejReason “Order exceeds limit”
INSUFFICIENT_BALANCE - maps to OrdRejReason “Order exceeds limit”
EXCHANGE_PAUSED - maps to OrdRejReason “Exchange closed”
TRADING_PAUSED - maps to OrdRejReason “Exchange closed”
INVALID_ORDER - maps to OrdRejReason “Unsupported order characteristic”
ORDER_GROUP_NOT_FOUND - maps to OrdRejReason “Unsupported order characteristic”
EXCEEDED_PER_MARKET_RISK_LIMIT - maps to OrdRejReason “Order exceeds limit”
EXCEEDED_SELL_POSITION_FLOOR - maps to OrdRejReason “Order exceeds limit”
CUSTOMER_ACCOUNT_NOT_FOUND - maps to OrdRejReason “Unknown account”
PERMISSION_DENIED_FOR_CUSTOMER_ACCOUNT - maps to OrdRejReason “Unknown account”
FOK_INSUFFICIENT_VOLUME - maps to ExecutionType “Canceled”
POST_ONLY_CROSS - maps to OrdRejReason “Other”
ORDER_GROUP_CANCEL - maps to ExecutionType “Canceled”
TAKER_CANCEL_FOR_SELF_TRADE_PREVENTION - maps to ExecutionType “Canceled”
MAKER_CANCEL_FOR_SELF_TRADE_PREVENTION - maps to ExecutionType “Canceled”
IMMEDIATE_OR_CANCELLED - maps to ExecutionType “Canceled”
​
OrderCancelReject (35=9)

Exchange-side amend and cancel failures are returned as OrderCancelReject (35=9), not ExecutionReport.
Text (58)	CxlRejReason (102)
INVALID_AMEND_QTY_FOR_ORDER	Broker
CANNOT_UPDATE_FILLED_ORDER	Broker
SELF_CROSS_ATTEMPT	Invalid price increment
​
Position and Fee Information

When ExecType=Trade:
Tag	Name	Description
704	LongQty	Net Yes position after trade as a decimal quantity
705	ShortQty	Net No position after trade as a decimal quantity
136	NoMiscFees	Number of fees
137	MiscFeeAmt	Total fees in dollars
138	MiscFeeCurr	Currency (USD)
139	MiscFeeType	Exchange Fees<4>
891	MiscFeeBasis	Fee unit (always ABSOLUTE<0>)
880	TrdMatchID	Unique trade identifier
1057	AggressorIndicator	Taker/Maker flag
​
Collateral Changes

Tag	Name	Description
1703	NoCollateralAmountChanges	Number of collateral changes
1704	CollateralAmountChange	Delta in dollars
1705	CollateralAmountType	BALANCE or PAYOUT
​
Party Information

Party fields from the original order request are echoed back in ExecutionReports:
Tag	Name	Description
453	NoPartyIDs	Number of parties (for sub-accounts)
448	PartyID	Sub-account identifier
452	PartyRole	Customer Account<24>
79	AllocAccount	Subaccount number (0-32)
Party and AllocAccount fields are only included when the order is placed under a sub-account. These fields help track orders across different sub-accounts or FCM clients.
​
Rejection Reasons (102)

Too late to cancel<0>: Order already filled
Unknown order<1>: Order not found
Other<99>: See Text field
​
Mass Cancel Request (35=q)

Cancel all orders for the trading session. Only available on KalshiNR (NewOrderMode) sessions.
Tag	Name	Description
11	ClOrderID	Unique request ID
530	MassCancelRequestType	Cancel for session<6>
​
Mass Cancel Report (35=r)

Response to mass cancel request.
Tag	Name	Description
11	ClOrderID	Request ID
37	OrderID	Operation ID
531	MassCancelResponse	Success<6> or Rejected<0>
532	MassCancelRejectReason	If rejected
Individual ExecutionReports will follow for each canceled order.
Session Management
Order Group Messages

FIX
Session Management
Managing FIX sessions including logon, logout, and message sequencing
​
Session Management

​
Creating FIX API Keys

​
Generate RSA Key Pair

First, generate a 2048 bit RSA PKCS#8 key pair:
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out kalshi-fix.key
openssl rsa -in kalshi-fix.key -pubout -out kalshi-fix.pub
This creates two files:
kalshi-fix.key: Your private key (keep secure, never share)
kalshi-fix.pub: Your public key (share with Kalshi)
​
Create API Key

Navigate to https://demo.kalshi.co/account/profile
Click the “Create key” button
Name your API Key
Copy and paste the contents of kalshi-fix.pub into the “RSA public key” field
Click “Create”
Copy the generated FIX API Key (UUID format)
Store your private key securely. It is equivalent to your username + password and should never be sent to anyone, including Kalshi employees.
​
Logon Process

​
Logon Message (35=A)

The initiator must send a Logon message to establish a session. The acceptor will either:
Respond with a Logon message acknowledging successful logon
Respond with a Logout (35=5) message if the logon fails
​
Required Fields

Tag	Name	Description	Value
98	EncryptMethod	Method of encryption	None<0>
96	RawData	Client logon message signature	Base64 encoded signature
108	HeartbeatInt	Heartbeat interval (seconds)	N > 3
1137	DefaultApplVerID	Default application version	FIX50SP2<9>
​
Optional Fields

Tag	Name	Description	Default
141	ResetSeqNumFlag	Reset sequence numbers on logon. Must be Y for KalshiNR, KalshiDC, KalshiPT.	N
108	HeartbeatInt	Heartbeat <0> interval (seconds)	30
8013	CancelOrdersOnDisconnect	Cancel orders on any disconnection (including graceful logout)	N
20126	ListenerSession	Listen-only session (KalshiNR/RT only, requires SkipPendingExecReports=Y)	N
20127	ReceiveSettlementReports	Receive settlement reports (KalshiRT only)	N
20200	MessageRetentionPeriod	How long session messages will be store for retransmission (KalshiRT and KalshiRFQ only), max of 72.	24
21005	UseDollars	Enable dollar-based price format for prices, including subpenny precision	N
21011	SkipPendingExecReports	Skip PENDING_{NEW|REPLACE|CANCEL} execution reports	N
21012	UseExpiredOrdStatus	Emit Expired<C> (150/39) for expiry-style system cancellations (CloseCancel and OrderExpiryCancel) instead of Canceled<4>	N
21007	EnableIocCancelReport	Partially filled IOC orders produce a cancel report	N
21008	PreserveOriginalOrderQty	OrderQty tag 38 always reflects original order quantity across all states	N
​
Signature Generation

The RawData field must contain a PSS RSA signature of the pre-hash string:
PreHashString = SendingTime + SOH + MsgType + SOH + MsgSeqNum + SOH + SenderCompID + SOH + TargetCompID
Critical: SendingTime in Signature
The SendingTime in the PreHashString must match exactly the value in field 52 of your logon message.
Using a FIX library: Most libraries auto-add SendingTime. Use that exact value (don’t manually add a timestamp) when generating the signature.
Building the message yourself: Include SendingTime in your message and use that same value in the PreHashString. Format: YYYYMMDD-HH:MM:SS.mmm

Python
from base64 import b64encode
from Cryptodome.Signature import pss
from Cryptodome.Hash import SHA256
from Cryptodome.PublicKey import RSA

# Load private key
private_key = RSA.import_key(open('kalshi-fix.key').read().encode('utf-8'))

# Build message string
# IMPORTANT: Use the EXACT SendingTime that will appear in field 52
# If constructing the message yourself: generate SendingTime and use it here
# If using a library: use the SendingTime value that your library will set
sending_time = "20230809-05:28:18.035"
msg_type = "A"
msg_seq_num = "1"
sender_comp_id = "your-fix-api-key-uuid"
target_comp_id = "Kalshi"  # Or appropriate TargetCompID

msg_string = chr(1).join([
    sending_time, msg_type, msg_seq_num,
    sender_comp_id, target_comp_id
])

# Generate signature
msg_hash = SHA256.new(msg_string.encode('utf-8'))
signature = pss.new(private_key).sign(msg_hash)
raw_data_value = b64encode(signature).decode('utf-8')
​
Session Maintenance

​
Heartbeat Protocol

Default interval: 30 seconds
Both sides must respond to TestRequest messages
Connection terminates if heartbeat response not received within interval
​
Message Sequence Numbers

​
Sequence Number Rules

Must be unique and increase by one for each message
Empty MsgSeqNum results in session termination
Lower than expected = serious failure, connection terminated
Higher than expected = recoverable with ResendRequest (if supported)
​
Reconnection Procedure

For unexpected sequence numbers:
Document the issue with logs for out-of-band communication
Check order status via REST API or UI
Establish new session with reset sequence numbers
​
Maintenance Window

Kalshi performs weekly scheduled maintenance every Thursday from 3:00 AM to 5:00 AM ET. During this window:
All FIX sessions are forcibly disconnected
Upon reconnecting after maintenance, all clients must reset sequence numbers to 1
You must send ResetSeqNumFlag=Y (tag 141) on your first Logon after maintenance, or connect to a session type that always resets (KalshiNR, KalshiDC, KalshiPT).
​
Resting Orders During Maintenance

When trading is paused for maintenance, resting orders are handled based on the CancelOrderOnPause tag:
Tag	Name	Value	Behavior
21006	CancelOrderOnPause	Y	Order is automatically cancelled when trading pauses
21006	CancelOrderOnPause	N (default)	Order remains resting on the book and resumes when trading reopens
Orders without CancelOrderOnPause=Y are not cancelled during maintenance. They remain on the book unchanged and become active again once trading resumes.
​
ResendRequest (35=2)

Only available on Order Entry with Retransmission (KalshiRT) sessions.
​
Limitations

Lookback window limited to 24 hours (or up to 72 hours if MessageRetentionPeriod was set on Logon request)
If you provide a BeginSeqNo that is beyond the lookback window, you will receive a Reject message
Tag	Name	Description
7	BeginSeqNo	Lower bound (inclusive)
16	EndSeqNo	Upper bound (inclusive)
​
Error Handling

​
Reject (35=3)

Sent when a message cannot be processed due to session-level rule violations.
Tag	Name	Description	Required
45	RefSeqNum	Sequence number of rejected message	Yes
58	Text	Human-readable error description	No
371	RefTagID	Tag number that caused reject	No
372	RefMsgType	MsgType of referenced message	No
373	SessionRejectReason	Rejection reason code	No
Reject indicates serious errors that may result from faulty logic. Log and investigate these errors.
​
BusinessMessageReject (35=j)

Sent for business logic violations rather than session-level errors.
Tag	Name	Description	Required
45	RefSeqNum	Sequence number of rejected message	Yes
58	Text	Human-readable error description	No
371	RefTagID	Tag number that caused reject	No
372	RefMsgType	MsgType of referenced message	No
379	BusinessRejectRefID	Business-level ID of rejected message	No
380	BusinessRejectReason	Business rejection reason code	Yes
​
Session Termination

​
Logout (35=5)

Graceful session termination:
Initiator sends Logout message
Acceptor responds with Logout (empty Text field)
Transport layer connection terminated
Tag	Name	Description
58	Text	Error description (if any)
If CancelOrdersOnDisconnect=Y, all orders are canceled even on graceful logout.
​
Message Headers

All messages must include standard FIX headers:
Tag	Name	Description	Requirements
8	BeginString	Protocol version	FIXT.1.1 (must be first)
9	BodyLength	Message length in bytes	Must be second
34	MsgSeqNum	Message sequence number	Unique, incrementing
35	MsgType	Message type	Must be third
52	SendingTime	UTC timestamp	Within 120 seconds of server time
​
Message Trailers

Tag	Name	Description	
10	CheckSum	Standard FIX checksum	Must be last field
CheckSum is calculated by summing ASCII values of all characters (except checksum field) modulo 256.
​
Best Practices

Session Configuration
Use unique ClOrdIDs across all message types
Implement proper heartbeat handling
Monitor sequence numbers carefully
Error Recovery
Implement automatic reconnection logic
Store order state locally for recovery
Use drop copy session for missed messages
Security
Rotate API keys periodically
Monitor for unauthorized access
Use secure storage for private keys
Connectivity
Order Entry Messages

FIX
Order Group Messages
Manage order groups for automatic position management
​
Order Group Messages

​
Overview

Order groups provide automatic order cancellation when a contracts limit is reached. This feature helps manage risk by ensuring positions don’t exceed predefined thresholds. Limits are evaluated over a rolling 15-second window. Order groups are managed through custom FIX message types.
When an order group’s contracts limit is exceeded, all orders in the group are automatically canceled and no new orders can be placed until the group is reset.
​
Order Group Request (35=UOG)

Manage order groups with Create, Reset, Delete, Trigger, and Update operations.
​
Required Fields

Tag	Name	Description	Type/Values
20131	OrderGroupAction	Operation to perform	Create<1>, Reset<2>, Delete<3>, Trigger<4>, Update<5>
​
Fields by Action

​
Create (Action=1)

Tag	Name	Description	Required
20132	OrderGroupContractsLimit	Maximum contracts allowed (1-1,000,000)	Yes
The OrderGroupID is generated by the server and returned in the response. Do not include tag 20130 in Create requests.
​
Reset (Action=2)

Tag	Name	Description	Required
20130	OrderGroupID	ID of group to reset	Yes
​
Delete (Action=3)

Tag	Name	Description	Required
20130	OrderGroupID	ID of group to delete	Yes
Deleting an order group cancels all resting orders in that group.
​
Trigger (Action=4)

Tag	Name	Description	Required
20130	OrderGroupID	ID of group to trigger	Yes
The Trigger action immediately cancels all orders in the specified order group, regardless of whether the contracts limit has been reached. This is useful for manual risk management or emergency order cancellation.
​
Update (Action=5)

Tag	Name	Description	Required
20130	OrderGroupID	ID of group to update	Yes
20132	OrderGroupContractsLimit	New maximum contracts allowed (1-1,000,000)	Yes
If the updated limit would immediately trigger the group (based on the rolling 15-second window), the server cancels all orders in the group and marks it as triggered. No new orders can be placed until the group is reset.
​
Example Messages

Create Order Group
8=FIXT.1.1|9=150|35=UOG|34=5|52=20230809-12:34:56.789|49=your-api-key|56=KalshiNR|
20131=1|20132=5000|10=123|
Reset Order Group
8=FIXT.1.1|9=150|35=UOG|34=6|52=20230809-12:34:57.789|49=your-api-key|56=KalshiNR|
20131=2|20130=770e8400-e29b-41d4-a716-446655440002|10=124|
Delete Order Group
8=FIXT.1.1|9=150|35=UOG|34=7|52=20230809-12:34:58.789|49=your-api-key|56=KalshiNR|
20131=3|20130=770e8400-e29b-41d4-a716-446655440002|10=125|
Trigger Order Group
8=FIXT.1.1|9=150|35=UOG|34=8|52=20230809-12:34:59.789|49=your-api-key|56=KalshiNR|
20131=4|20130=770e8400-e29b-41d4-a716-446655440002|10=126|
Update Order Group Limit
8=FIXT.1.1|9=150|35=UOG|34=9|52=20230809-12:35:00.789|49=your-api-key|56=KalshiNR|
20131=5|20130=770e8400-e29b-41d4-a716-446655440002|20132=2500|10=127|
​
Order Group Response (35=UOH)

Response to order group management requests.
​
Response Fields

Tag	Name	Description
20130	OrderGroupID	ID of the order group
Business-logic errors (e.g. order group not found, exchange-returned errors) are returned as BusinessMessageReject (35=j) messages. Malformed fields (e.g. invalid UUID format for OrderGroupID) produce a session-level Reject (35=3).

FIX
RFQ Messages
Request for Quote functionality for RFQ creators and market makers
​
RFQ (Request for Quote) Messages

​
Overview

RFQ functionality involves two types of participants connecting via different FIX sessions:
RFQ Creators - Users who want to trade via RFQ (connect via RT mode):
Create RFQ via QuoteRequest (35=R)
Receive quotes from market makers via Quote (35=S)
Accept a quote via AcceptQuote (35=UA)
Receive trade execution via ExecutionReport (35=8)
Market Makers - Users who provide quotes (connect via RfqMode):
Receive QuoteRequest from exchange
Respond with Quote (35=S)
Receive acceptance notification
Confirm execution via QuoteConfirm (35=U7)
RFQ Creators use the KalshiRT endpoint (same as order entry), which provides message persistence and retransmission support. Market Makers use the KalshiRFQ endpoint to receive RFQ broadcasts and submit quotes.
​
Message Flow

​
Full RFQ Flow (Creator via FIX)









Market Maker
Exchange
RFQ Creator
Market Maker
Exchange
RFQ Creator
QuoteRequest (35=R)
QuoteRequestAck (35=b)
QuoteRequest (35=R)
Quote (35=S)
QuoteStatusReport (35=AI)
Status=PENDING
Quote (35=S)
AcceptQuote (35=UA)
AcceptQuoteStatus (35=UC)
QuoteStatusReport (35=AI)
Status=ACCEPTED
QuoteConfirm (35=U7)
QuoteConfirmStatus (35=U8)
ExecutionReport (35=8)
​
QuoteRequest (35=R)

This message is used bidirectionally:
Creator → Exchange: Create a new RFQ
Exchange → Market Makers: Notify of new RFQ
​
Creator → Exchange (Create RFQ)

Tag	Name	Type	Required	Description
131	QuoteReqId	UUID	Y	Client-assigned RFQ identifier
146	NoRelatedSym	Integer	Y	Number of symbols (must be 1)
55	Symbol	String	C	Market ticker. Required unless MVE legs are specified
38	OrderQty	Decimal	C	Number of contracts as a fixed-point decimal. Currently only whole contracts are accepted (for example 5, 5.0, or 5.00). Required unless CashOrderQty is specified
152	CashOrderQty	Decimal	C	Target cost in dollars. Required unless OrderQty is specified
453	NoPartyIDs	Integer	N	Number of parties (only 1 supported)
448	PartyId	String	N	FCM SubtraderId for the customer on whose behalf the RFQ is submitted
452	PartyRole	Integer	N	24 (CustomerAccount) - required when using PartyId
21015	RestRemainder	Char	N	Y/N - Allow partial fills (default: N)
21016	ReplaceExisting	Char	N	Y/N - Whether to delete existing RFQs as part of this RFQ’s creation (default: N)
20180	MultivariateCollectionTicker	String	C	Collection ticker for parlay/MVE markets. Use instead of Symbol
20181	NoMultivariateSelectedLegs	Integer	C	Number of MVE legs (repeating group). Required with 20180
20182	MultivariateSelectedEventTicker	String	Y	Event ticker for the leg
20183	MultivariateSelectedMarketTicker	String	Y	Market ticker for the leg
20184	MultivariateSelectedMarketSide	String	Y	Side for the leg (“yes” or “no”)
MVE/Parlay Support: Instead of specifying a Symbol, you can submit MVE legs directly. The server will automatically resolve or create the parlay market and return the resolved market ticker in the QuoteRequestAck.
​
Exchange → Market Maker (RFQ Notification)

Tag	Name	Type	Required	Description
131	QuoteReqId	UUID	Y	Server-assigned RFQ identifier
146	NoRelatedSym	Integer	Y	Number of symbols (always 1)
55	Symbol	String	Y	Market ticker
38	OrderQty	Decimal	Y	Number of contracts as a fixed-point decimal. Currently emitted as whole contracts
152	CashOrderQty	Decimal	N	Target cost in dollars (if specified by creator)
453	NoPartyIDs	Integer	N	Number of parties (always 1)
448	PartyId	String	N	Requester public communications ID. This value is pseudonymous and is not the requester’s SubtraderId
20180	MultivariateCollectionTicker	String	N	Collection ticker for multivariate markets
20181	NoMultivariateSelectedLegs	Integer	N	Number of MVE legs (repeating group)
20182	MultivariateSelectedEventTicker	String	N	Event ticker for the leg
20183	MultivariateSelectedMarketTicker	String	N	Market ticker for the leg
20184	MultivariateSelectedMarketSide	String	N	Side for the leg (“yes” or “no”)
​
QuoteRequestAck (35=b)

Exchange response to an inbound QuoteRequest from an RFQ creator.
Tag	Name	Type	Required	Description
131	QuoteReqId	UUID	Y	Client-assigned RFQ ID (echoed back)
303	QuoteRequestType	Integer	Y	1 (MANUAL)
21023	RfqId	UUID	Y	Server-assigned RFQ ID
55	Symbol	String	C	Resolved market ticker. Present when MVE legs were submitted
The server-assigned RFQ ID is returned in tag 21023. Store it if you want to reconcile later Quote or QuoteStatusReport messages to the created RFQ. For RFQCancel, use your original client-assigned QuoteReqId (tag 131).
When creating an RFQ with MVE legs instead of a Symbol, the resolved market ticker is returned in tag 55. This is the market that was created or looked up based on your leg selection.
​
Quote (35=S)

This message is used bidirectionally:
Market Maker → Exchange: Submit a quote for an RFQ
Exchange → Creator: Notify creator of a new quote
If a new Quote is created when an existing quote for the same market already exists for the user, the exchange will cancel the existing quote.
​
Market Maker → Exchange (Submit Quote)

Tag	Name	Type	Required	Description
117	QuoteId	UUID	Y	Client-assigned quote identifier
131	QuoteReqId	UUID	Y	Server-assigned RFQ ID (from QuoteRequest)
55	Symbol	String	Y	Market ticker
132	BidPx	Integer	C	Yes price in cents (1-99)
133	OfferPx	Integer	C	No price in cents (1-99)
79	AllocAccount	Integer	N	Subaccount number (0-32). If provided, the quote will be created for the specified subaccount.
​
Exchange → Creator (Quote Notification)

Tag	Name	Type	Required	Description
117	QuoteId	UUID	Y	Quote identifier (use this to accept)
131	QuoteReqId	UUID	Y	Server-assigned RFQ ID
55	Symbol	String	Y	Market ticker
132	BidPx	Decimal	C	Yes price in dollars (e.g. 0.4500). Not present when zero
133	OfferPx	Decimal	C	No price in dollars (e.g. 0.5500). Not present when zero
38	OrderQty	Decimal	N	Number of contracts as a fixed-point decimal. Currently emitted as whole contracts
Either BidPx or OfferPx can be zero, but not both. Zero indicates no quote for that side.
​
QuoteStatusReport (35=AI)

A QuoteStatusReport is sent by the exchange:
In response to a Quote. Status will be PENDING if processed, or REJECTED if rejected
When the requester accepts the quote. Status will be ACCEPTED
In response to a QuoteCancel. Status will be CANCELLED
Tag	Name	Type	Required	Description
117	QuoteId	String	Y	Quote identifier (empty if rejected)
131	QuoteReqId	String	Y	Request reference
79	AllocAccount	Integer	C	Subaccount number (0-32). Present if the quote was created for a subaccount
297	QuoteStatus	Integer	Y	Current status
38	OrderQty	Decimal	C	Quantity of contracts as a fixed-point decimal. Currently emitted as whole contracts. Not present if REJECTED
132	BidPx	Integer	C	Yes price in cents. Only integer part considered. Not present if REJECTED
133	OfferPx	Integer	C	No price in cents. Only integer part considered. Not present if REJECTED
54	AcceptedSide	Char	C	Side accepted (1=Yes, 2=No). Only present if ACCEPTED
58	Text	String	C	Rejection reason. Only present if REJECTED
​
Quote Status Values (297)

ACCEPTED<0>: Requester accepted the quote
REJECTED<5>: Exchange rejected the quote
PENDING<10>: Quote processed, awaiting action
CANCELLED<17>: Quote cancelled
​
QuoteCancel (35=Z)

Market maker cancels an active quote.
Tag	Name	Type	Required	Description
117	QuoteId	String	Y	Quote to cancel
Exchange responds with QuoteStatusReport (Status=CANCELLED).
​
QuoteCancelStatus (35=U9)

Response to QuoteCancel from exchange.
Tag	Name	Type	Required	Description
117	QuoteId	String	Y	Quote identifier
298	QuoteCancelStatus	Integer	Y	CANCELED(0) or REJECTED(1)
58	RejectReason	String	C	Present if QuoteCancelStatus is REJECTED
​
QuoteConfirm (35=U7)

Market maker confirms willingness to execute after quote acceptance.
Tag	Name	Type	Required	Description
117	QuoteId	String	Y	Accepted quote ID
Quote must be confirmed within 30 seconds of acceptance or it will be voided.
​
QuoteConfirmStatus (35=U8)

Exchange response to quote confirmation.
Tag	Name	Type	Required	Description
117	QuoteId	String	Y	Quote identifier
21010	QuoteConfirmStatus	Integer	Y	ACCEPTED(0) or REJECTED(1)
58	RejectReason	String	C	Present if QuoteConfirmStatus is REJECTED
​
AcceptQuote (35=UA)

RFQ creator accepts a quote from a market maker.
Tag	Name	Type	Required	Description
117	QuoteId	UUID	Y	Quote to accept
54	Side	Char	Y	FIX side (1=BUY, 2=SELL). For AcceptQuote, BUY accepts the maker’s NO quote and SELL accepts the maker’s YES quote.
38	OrderQty	Decimal	N	Contracts to accept as a fixed-point decimal. Currently only whole contracts are accepted
11	ClOrdID	String	N	Client order ID
453	NoPartyIDs	Integer	N	Number of parties (only 1 supported)
448	PartyId	String	N	FCM SubtraderId for the customer on whose behalf the accept is submitted
452	PartyRole	Integer	N	24 (CustomerAccount) for SubtraderId
​
AcceptQuoteStatus (35=UC)

Exchange response to AcceptQuote.
Tag	Name	Type	Required	Description
117	QuoteId	String	Y	Quote identifier
21025	AcceptQuoteStatus	Integer	Y	ACCEPTED(0) or REJECTED(1)
58	Text	String	C	Rejection reason if REJECTED
​
RFQCancel (35=UE)

RFQ creator cancels/deletes an active RFQ.
Tag	Name	Type	Required	Description
131	QuoteReqId	UUID	Y	Client-assigned RFQ ID (from original QuoteRequest)
Use your original client-assigned QuoteReqId from the QuoteRequest message. The RFQ ID already identifies the associated subtrader, so no PartyID fields are needed. When an RFQ is cancelled, market makers receive a QuoteRequestReject (35=AG) notification.
​
RFQCancelStatus (35=UB)

Exchange response to RFQCancel.
Tag	Name	Type	Required	Description
131	QuoteReqId	String	Y	RFQ identifier (echoes back the ID from RFQCancel request)
21013	RFQCancelStatus	Integer	Y	CANCELED(0) or REJECTED(1)
58	Text	String	C	Rejection reason if REJECTED
​
QuoteRequestReject (35=AG)

Exchange notifies that a quote request was cancelled.
Tag	Name	Type	Required	Description
58	Text	String	Y	Reason the quote has been cancelled
131	QuoteReqId	String	Y	Request identifier
658	QuoteRequestRejectReason	Integer	Y	OTHER(99)
Market makers do not send QuoteRequestReject when ignoring a request.
​
Best Practices

​
For RFQ Creators

RFQ ID Management
Store the server-assigned RFQ ID from QuoteRequestAck if you want to reconcile later Quote or QuoteStatusReport messages to the created RFQ
Use your original client-assigned QuoteReqId for RFQCancel
Quote Selection
Compare quotes from multiple market makers
Accept quotes promptly as they may expire
Verify price and quantity before accepting
Cancellation
Cancel RFQs you no longer need
Wait for RFQCancelStatus before considering cancelled
​
For Market Makers

Response Time
Respond to quote requests promptly
Confirm accepted quotes within 30 seconds
Cancel stale quotes proactively
Quote Management
Track active quotes locally
Handle quote replacements properly
Monitor for acceptance notifications
Risk Management
Validate prices before quoting
Implement position limits
Handle partial quotes (one-sided)
​
Error Handling

Rejection Scenarios
Invalid price range
Symbol not found
Insufficient balance
Technical issues
Timeout Handling
30-second confirmation window
Automatic quote expiration
Network disconnection recovery
​
Example Workflow

​
RFQ Creator Flow


Create RFQ (Creator → Exchange)

Create RFQ with MVE Legs (Creator → Exchange)

QuoteRequestAck (Exchange → Creator)

QuoteRequestAck with Resolved Ticker (Exchange → Creator)

Quote Notification (Exchange → Creator)

Accept Quote (Creator → Exchange)

AcceptQuoteStatus (Exchange → Creator)

Cancel RFQ (Creator → Exchange)

RFQCancelStatus (Exchange → Creator)
8=FIXT.1.1|35=R|131=client-req-123|146=1|38=100|55=HIGHNY-23DEC31|
​
Market Maker Flow


QuoteRequest (Exchange → MM)

Quote Response (MM → Exchange)

Quote Status Pending (Exchange → MM)

Quote Accepted (Exchange → MM)

Quote Confirmation (MM → Exchange)

QuoteConfirmStatus (Exchange → MM)
8=FIXT.1.1|35=R|131=server-rfq-456|146=1|38=100|55=HIGHNY-23DEC31|453=1|448=anon-456|
​
Integration Notes

RFQ Creators use the KalshiRT endpoint (RT mode) - same connection as order entry, with message persistence and retransmission support
Market Makers use the KalshiRFQ endpoint (RfqMode) to receive RFQ broadcasts and submit quotes
ExecutionReport (35=8) is sent to the RFQ creator after trade execution
RFQs expire after 24 hours if not cancelled or accepted

FIX
Drop Copy Session
Recover missed execution reports and query historical order events
​
Drop Copy Session

​
Overview

The drop copy session provides an alternative method to query for missed execution reports without using the FIX retransmission protocol. This is particularly useful for:
Recovering from connection failures
Auditing order activity
Building backup systems
Compliance recording
Kalshi’s DropCopy format utilizes a request-response message type, if you are interested in a session that “follows along” the execution report activity of your trading session, consider using a KalshiRT session with the ListenerFlag parameter.
​
Connection Details

Environment	URL	Port	TargetCompID
Production	fix.elections.kalshi.com	8229	KalshiDC
Demo	fix.demo.kalshi.co	8229	KalshiDC
​
EventResendRequest (35=U1)

Request execution reports within a specified ExecID range.
​
Fields

Tag	Name	Description	Required
21001	BeginExecID	Starting ExecID (inclusive)	Yes
21002	EndExecID	Ending ExecID (inclusive)	No
If EndExecID is not provided, it defaults to the latest ExecID in your history.
​
Limitations

Lookback Window: Last 3 hours only
Message Types: Only ExecutionReport (35=8) supported
Excluded Messages:
Rejects (no valid ExecID)
Pending new orders (ExecID = “-1;-1”)
​
Example Request

8=FIXT.1.1|35=U1|21001=12345;67890|21002=12350;67895|
​
EventResendComplete (35=U2)

Sent after all requested events have been resent.
​
Fields

Tag	Name	Description	Required
45	RefSeqNum	MsgSeqNum of the EventResendRequest	Yes
21003	ResentEventCount	Total number of events resent	Yes
​
EventResendReject (35=U3)

Sent when a resend request cannot be fulfilled.
​
Fields

Tag	Name	Description	Required
45	RefSeqNum	MsgSeqNum of the EventResendRequest	Yes
21004	EventResendRejectReason	Rejection code	Yes
​
Rejection Reasons (21004)

Code	Description
1	Too many resend requests
2	Server error
3	BeginExecID is too small (outside window)
4	EndExecID is too large
​
Usage Patterns

​
Recovery After Disconnect

1
Track Last ExecID

Store the last processed ExecID before disconnect
2
Reconnect to Drop Copy

Establish new drop copy session
3
Request Missing Events

Send EventResendRequest starting from last ExecID
4
Process Resent Events

Handle execution reports with new sequence numbers
​
Best Practices

​
1. ExecID Management

Store ExecIDs persistently
Handle the two-part format correctly (e.g., “12345;67890”)
Implement proper ID comparison logic
​
2. Rate Limiting

Avoid excessive resend requests
Implement exponential backoff on failures
Batch requests when possible
​
3. Deduplication

Events may arrive via both primary and drop copy sessions
Implement deduplication based on ExecID
Handle out-of-order delivery
​
4. Error Recovery

Handle all rejection codes appropriately
Implement retry logic with delays
Alert on persistent failures
​
Comparison with Retransmission

Feature	Drop Copy	Retransmission
Session Type	Separate	Same as order entry
Sequence Numbers	Independent	Original preserved
Lookback Window	3 hours	3 hours
Message Types	ExecutionReport only	All types
Use Case	Recovery/Audit	Real-time gaps
All resent messages will have new FIX sequence numbers in the drop copy session, different from their original sequence numbers in the order entry session.


Market Settlement
Settlement reports for market outcomes and position resolution
​
Market Settlement

​
Overview

Market settlement messages provide information about market outcomes and the resulting position settlements. These messages are available on:
KalshiPT (Post Trade) sessions
KalshiRT sessions when ReceiveSettlementReports=Y in Logon
Settlement occurs when a market’s outcome is determined, triggering automatic position resolution and fund transfers.
​
Market Settlement Report (35=UMS)

Provides settlement details for a specific market.
​
Message Structure

Tag	Name	Description	Required
20105	MarketSettlementReportID	Unique settlement identifier	Yes
55	Symbol	Market ticker (e.g., NHIGH-23JAN02-66)	Yes
715	ClearingBusinessDate	Date settlement cleared (YYYYMMDD)	Yes
20106	TotNumMarketSettlementReports	Total number of settlement reports in sequence	No
20107	MarketResult	Result of the market when determined	Yes
893	LastFragment	Last page indicator (Y/N)	No
730	SettlementPrice	Settlement price of market in cents (2 decimal places, e.g. 30.60)	Yes
​
Repeating Groups

​
Party Information (NoMarketSettlementPartyIDs)

Tag	Name	Description
20108	NoMarketSettlementPartyIDs	Number of parties
20109	MarketSettlementPartyID	Unique identifier for party
20110	MarketSettlementPartyRole	Type of party (Customer Account<24>)
704	LongQty	Decimal quantity of YES position held
705	ShortQty	Decimal quantity of NO position held
​
Collateral Changes (NoCollateralAmountChanges)

Tag	Name	Description
1703	NoCollateralAmountChanges	Number of collateral changes (should be only 1 - payout balance change)
1704	CollateralAmountChange	Delta in dollars
1705	CollateralAmountType	Balance<1> or Payout<2>
​
Fees (NoMiscFees)

Tag	Name	Description
136	NoMiscFees	Number of fee entries (always 1)
137	MiscFeeAmt	Fee amount in dollars (zero when no fee)
138	MiscFeeCurr	Currency (USD)
139	MiscFeeType	Type of fee (Exchange fees<4>)
891	MiscFeeBasis	Unit for fee (Absolute<0>)
​
Settlement Process

​
Market Resolution Flow









Market Expires
Outcome Determined
Settlement Report Generated
Positions Resolved
Funds Transferred
Yes Holders: Receive $1
No Holders: Receive $0
​
Settlement Calculations

For each position:
Yes outcome: Yes contract holders receive $1 per contract
No outcome: No contract holders receive $1 per contract
Net position: Only net positions are settled (after netting)
​
Example Settlement Report

// Market settled as "Yes", no fees
8=FIXT.1.1|35=UMS|
20105=settle-123|55=HIGHNY-23DEC31|715=20231231|
20107=Yes|
20108=1|
  20109=user-456|20110=24|
  704=100|705=0|
  1703=1|
    1704=100.00|1705=1|
  136=1|
    137=0.00|138=USD|139=4|891=0|
893=Y|
// Market settled as "Yes", with sub-cent rounding fee
8=FIXT.1.1|35=UMS|
20105=settle-456|55=HIGHNY-23DEC31|715=20231231|
20107=Yes|
20108=1|
  20109=user-789|20110=24|
  704=100|705=0|
  1703=1|
    1704=100.00|1705=1|
  136=1|
    137=0.006|138=USD|139=4|891=0|
893=Y|
The first example shows:
Market HIGHNY-23DEC31 settled as “Yes”
User held 100 Yes contracts
Received $100.00 payout to balance
Zero settlement fees
The second example shows:
Same market, different user
100.00
p
a
y
o
u
t
w
i
t
h
a
100.00payoutwitha0.006 rounding fee
​
Pagination

Large settlement batches may span multiple messages:
Tag	Use Case
20106	Total number of reports in batch
893	LastFragment=N for more pages, Y for last
Important: The MarketSettlementReportID (tag 20105) will be different across paginated responses. Each page of results generates a new unique settlement ID. Use the Symbol (tag 55) ticker to identify fragments belonging to the same paginated settlement.
​
Settlement Timing

Markets typically settle shortly after expiration, but timing can vary based on:
Market type
Data source availability
Manual review requirements
​
Integration Considerations

​
1. Position Reconciliation

def reconcile_settlement(report):
    # Verify position matches records
    our_position = get_position(report.Symbol)

    if report.LongQty != our_position.yes_contracts:
        alert("Position mismatch", report)

    # Verify payout calculation
    if report.MarketResult == "Yes":
        expected_payout = report.LongQty * 100  # cents
    else:
        expected_payout = report.ShortQty * 100

    if report.CollateralAmountChange != expected_payout:
        alert("Payout mismatch", report)
​
2. Multi-Account Handling

For sessions managing multiple accounts:
Each party (sub-account) receives separate entry
Aggregate by MarketSettlementPartyID
Track settlements per account
​
3. Fee Processing

Fees ensure that CollateralAmountChange (the actual payout) is in whole cents, since balances are tracked in cents. Settlement fees will be zero for simple yes / no determinations but will be applied under edge case scenarios like sub-cent scalar settlement.
When processing settlement reports:
Parse the NoMiscFees group for fee amounts
Account for fees (including negative rebates) in P&L calculations
CollateralAmountChange + MiscFeeAmt equals the pre-rounding settlement value
​
Best Practices

​
Real-time Processing

1
Subscribe to Reports

Set ReceiveSettlementReports=Y in KalshiRT Logon
2
Process Immediately

Update positions and balances in real-time
3
Reconcile

Compare with expected outcomes and positions
4
Update Risk

Adjust risk calculations for settled positions
​
Batch Processing

For post-trade reconciliation:
Connect to KalshiPT session
Query for day’s settlements
Process in sequence order
Generate settlement reports
​
Related Systems

System	Purpose
Order Entry	Track positions leading to settlement
Drop Copy	Audit trail of trades
Market Data	Market expiration times
REST API	Query market details and outcomes
​
Error Scenarios

​
Missing Settlements

If settlements are missing:
Check connection to appropriate session
Verify ReceiveSettlementReports flag
Use REST API as backup data source
Contact support if discrepancies persist
​
Incorrect Positions

Position mismatches may indicate:
Missed execution reports
Incorrect position tracking
Late trades near expiration
Always maintain independent position tracking for verification.

FIX
Error Handling
Understanding and handling errors in the FIX protocol
​
Error Handling

​
Overview

Kalshi FIX API uses standard FIX error messages with additional detail in the Text field. Errors fall into two categories:
Session-level errors: Protocol violations, handled with Reject (35=3)
Business-level errors: Application logic issues, handled with BusinessMessageReject (35=j) or specific rejection messages
​
Error Message Types

​
Reject (35=3)

Used for session-level protocol violations.
Tag	Name	Description	Required
45	RefSeqNum	Sequence number of rejected message	Yes
58	Text	Human-readable error description	No
371	RefTagID	Tag that caused the rejection	No
372	RefMsgType	Message type being rejected	No
373	SessionRejectReason	Rejection reason code	No
​
Session Reject Reasons (373)

Code	Reason	Description
0	Invalid tag number	Unknown tag in message
1	Required tag missing	Mandatory field not present
2	Tag not defined for message	Tag not valid for this message type
3	Undefined tag	Tag number not in FIX specification
4	Tag without value	Empty tag value
5	Incorrect value	Invalid value for tag
6	Incorrect data format	Wrong data type
7	Decryption problem	Security issue
8	Signature problem	Authentication failure
9	CompID problem	SenderCompID/TargetCompID issue
10	SendingTime accuracy	Time outside acceptable window
11	Invalid MsgType	Unknown message type
​
BusinessMessageReject (35=j)

Used for application-level business logic errors.
Tag	Name	Description	Required
45	RefSeqNum	Sequence number of rejected message	Yes
58	Text	Human-readable error description	No
371	RefTagID	Tag that caused the rejection	No
372	RefMsgType	Message type being rejected	No
379	BusinessRejectRefID	Business ID from rejected message	No
380	BusinessRejectReason	Business rejection reason code	Yes
​
Business Reject Reasons (380)

Code	Reason	Description
0	Other	See Text field for details
1	Unknown ID	Referenced ID not found
2	Unknown Security	Invalid symbol
3	Unsupported Message Type	Message type not implemented
4	Application not available	System temporarily unavailable
5	Conditionally required field missing	Context-specific field missing
​
Order-Specific Rejections

​
Order Reject Reasons (103)

In ExecutionReport (35=8) with ExecType=Rejected:
Code	Reason	Common Causes
1	Unknown symbol	Invalid market ticker
2	Exchange closed	Outside trading hours
3	Order exceeds limit	Position or order size limit, insufficient balance
4	Too late to enter	Market expired/closed
6	Duplicate order	ClOrdID already used
11	Unsupported order characteristic	Invalid order parameters, order ID/side/ticker mismatch on amend
13	Incorrect quantity	Invalid order size
99	Other	See Text field
​
Cancel Reject Reasons (102)

In OrderCancelReject (35=9):
Code	Reason	Description
0	Too late to cancel	Order already filled
1	Unknown order	Order ID not found, order ID/side/ticker mismatch
99	Other	See Text field
​
Common Error Scenarios

​
Example 1: Invalid Tag

Scenario: Undefined tag in NewOrderSingle
// Sent
8=FIXT.1.1|35=D|11=123|38=10|333333=test|...

// Response: Reject
8=FIXT.1.1|35=3|45=5|58=Undefined tag received|371=333333|372=D|373=3|
​
Example 2: Order Rejected by Exchange

Scenario: Trading during maintenance
// Sent
8=FIXT.1.1|35=D|11=456|38=10|55=HIGHNY-23DEC31|...

// Response: ExecutionReport (Rejected)
8=FIXT.1.1|35=8|11=456|150=8|39=8|58=EXCHANGE_PAUSED|103=2|...
Order-entry failures returned by the exchange are sent as ExecutionReport (35=8) with ExecType=Rejected, not as BusinessMessageReject. BusinessMessageReject (35=j) is used for application-layer failures before normal exchange rejection handling, such as rate limiting or listener-session restrictions.
​
Example 3: Order Rejection

Scenario: Insufficient balance
// Response: ExecutionReport
8=FIXT.1.1|35=8|11=789|150=8|39=8|58=INSUFFICIENT_BALANCE|103=3|...
​
Error Handling Best Practices

​
1. Comprehensive Logging

def handle_message(msg):
    if msg.type == 'Reject':
        log.error(f"Session reject: {msg.Text} (Tag: {msg.RefTagID}, Reason: {msg.SessionRejectReason})")
    elif msg.type == 'BusinessMessageReject':
        log.error(f"Business reject: {msg.Text} (Reason: {msg.BusinessRejectReason})")
    elif msg.type == 'ExecutionReport' and msg.ExecType == 'Rejected':
        log.error(f"Order rejected: {msg.Text} (Reason: {msg.OrdRejReason})")
​
2. Retry Strategies

Error Type	Retry Strategy
Session errors	Fix protocol issue before retry
Rate limit	Exponential backoff
Exchange closed	Wait for market open
Insufficient balance	Check balance before retry
Unknown symbol	Verify symbol, don’t retry
​
3. Graceful Degradation

1
Identify Error Type

Distinguish between recoverable and non-recoverable errors
2
Apply Appropriate Action

Recoverable: Implement retry with backoff
Non-recoverable: Alert and halt
3
Monitor and Alert

Track error rates and patterns for system health
​
Specific Error Conditions

​
Authentication Errors

Symptom	Likely Cause	Resolution
Logon rejected	Invalid signature	Check RSA key and signature algorithm
CompID problem	Wrong API key	Verify SenderCompID matches API key
Time accuracy	Clock skew	Sync system time with NTP
​
Order Entry Errors

Error	Check	Action
Unknown symbol	Symbol format	Use exact ticker from market data
Order exceeds limit	Position limits / available balance	Query current position and balance
Duplicate ClOrdID	ID generation	Ensure UUID uniqueness
Invalid price	Price range	Ensure (0, 100) cents with valid tick interval
​
Connection Errors

Connection errors often manifest as:
Heartbeat timeout
Sequence number gaps
Socket disconnection
Always implement reconnection logic with appropriate delays.
​
Error Response Patterns

​
Synchronous Errors

Immediate response to invalid request:
Request → Validation → Immediate Error Response
​
Asynchronous Errors

Delayed errors during processing:
Request → Initial Accept → Processing → Later Error Report

FIX
Subpenny Pricing
Dollar-based pricing format for subpenny precision
For the general overview of fixed-point pricing and contract quantities across REST and WebSocket APIs, see Fixed-Point Migration.
​
Technical Specification

To enable subpenny precision, include tag 21005 in your Logon message:
Tag	Name	Description	Value
21005	UseDollars	Enable dollar-based price format	Y
Overview:
Legacy Format (Cents): Prices given in whole cents. E.g. 72 cents = 72.
New Format (Dollars): Prices normalized to dollars with fixed precision (up to 4 decimal places).
Examples:
Cents	FIX Decimal	String Representation
1.23¢	Decimal(123, -4)	0.0123
72.5¢	Decimal(7250, -4)	0.725
99¢	Decimal(9900, -4)	0.99
Affected Tags:
Tag	Field Name	Description
6	AvgPx	Average price of fills
31	LastPx	Price of last fill
44	Price	Order limit price
132	BidPx	Quote bid price
133	OfferPx	Quote ask price
​
Sample Messages


logon
8=FIXT.1.1|9=300|35=A|34=1|52=20250926-21:54:07.001|
96=QhA8659Mhygcm+xE/wb1m...|21005=Y|
                            ^^ Enable dollar format

new order single
8=FIXT.1.1|9=200|35=D|34=2|52=20250926-21:54:16.040|
38=100.0|40=2|44=0.7500|54=1|60=20250926-21:54:16.040|
              ^^ price
10=092|

execution report
8=FIXT.1.1|9=400|35=8|34=4|52=20250926-21:54:16.159|
6=0.6600|14=100|31=0.7000|32=60|38=100.0000|39=2|44=0.7500|
^^ avgPx        ^^ lastPx                        ^^ price

WebSocket Connection
Main WebSocket connection endpoint. All communication happens through this single connection. Authentication is required to establish the connection; include API key headers during the WebSocket handshake. Some channels carry only public market data, but the connection itself still requires authentication. Use the subscribe command to subscribe to specific data channels. For more information, see the Getting Started guide.
WSS

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Bindings
method
type:
string
GET

Send
Subscribe Command
type:
object

show 3 properties
Subscribe to one or more channels
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
cmd
type:
string
required
subscribe
params
type:
object
required

show 9 properties
channels
type:
array
List of channels to subscribe to
market_ticker
type:
string
Subscribe to a single market. Type: string. Example: "KXBTCD-25AUG0517-T114999.99" (mutually exclusive with market_tickers)
market_tickers
type:
array
Subscribe to multiple markets. Type: array of strings. Example: ["KXBTCD-25AUG0517-T114999.99", "KXETHD-25AUG0517-T3749.99"] (mutually exclusive with market_ticker)
market_id
type:
string
Subscribe to a single market by UUID (ticker only; mutually exclusive with market_ids and market_ticker(s))
market_ids
type:
array
Subscribe to multiple markets by UUID (ticker only; mutually exclusive with market_id and market_ticker(s))
send_initial_snapshot
type:
boolean
If true, receive an initial snapshot for requested market tickers on the ticker channel
skip_ticker_ack
type:
boolean
If true, OK responses omit the market_tickers/market_ids lists for this subscription
shard_factor
type:
integer
Number of shards for communications channel fanout (optional)
shard_key
type:
integer
Shard key for communications channel fanout (requires shard_factor)
Unsubscribe Command
type:
object

show 3 properties
Cancel one or more subscriptions
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
cmd
type:
string
required
unsubscribe
params
type:
object
required

show 1 property
sids
type:
array
List of subscription IDs to cancel
List Subscriptions Command
type:
object

show 2 properties
List all active subscriptions
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
cmd
type:
string
required
list_subscriptions
Update Subscription - Add Markets
type:
object

show 3 properties
Add markets to an existing subscription
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
cmd
type:
string
required
update_subscription
params
type:
object
required

show 8 properties
sid
type:
integer
Server-generated subscription identifier (sid) used to identify the channel
sids
type:
array
Array containing exactly one subscription ID (alternative to sid). Either sid or sids must be provided, not both.
market_ticker
type:
string
Add/remove a single market. Type: string
market_tickers
type:
array
Add/remove multiple markets. Type: array of strings
market_id
type:
string
Add/remove a single market by UUID (ticker only)
market_ids
type:
array
Add/remove multiple markets by UUID (ticker only)
send_initial_snapshot
type:
boolean
If true, receive an initial snapshot for newly added market tickers on the ticker channel
action
type:
enum
Available options: add_markets, delete_markets
Update Subscription - Delete Markets
type:
object

show 3 properties
Remove markets from an existing subscription
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
cmd
type:
string
required
update_subscription
params
type:
object
required

show 8 properties
Update Subscription - Single SID Format
type:
object

show 3 properties
Update subscription using sid parameter instead of sids array
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
cmd
type:
string
required
update_subscription
params
type:
object
required

show 8 properties

Receive
Subscribed Response
type:
object

show 3 properties
Confirmation that subscription was successful
id
type:
integer
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
type
type:
string
required
subscribed
msg
type:
object
required

show 2 properties
Unsubscribed Response
type:
object

show 4 properties
Confirmation that unsubscription was successful
id
type:
integer
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
seq
type:
integer
required
Sequential number that should be checked if you want to guarantee you received all the messages. Used for snapshot/delta consistency
type
type:
string
required
unsubscribed
OK Response
type:
object

show 5 properties
Successful update operation response
id
type:
integer
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
sid
type:
integer
Server-generated subscription identifier (sid) used to identify the channel
seq
type:
integer
Sequential number that should be checked if you want to guarantee you received all the messages. Used for snapshot/delta consistency
type
type:
string
required
ok
msg
type:
object

show 2 properties
List Subscriptions Response
type:
object

show 3 properties
Response containing all active subscriptions
id
type:
integer
required
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
type
type:
string
required
ok
msg
type:
array
required
List of active subscriptions
Error Response
type:
object

show 3 properties
Error response for failed operations
id
type:
integer
Unique ID of the command request. Generated by the client and should be unique within a WS session. The simplest way to use it would be to start from 1 and then increment the value for every new command sent to the server. If the id is set to 0, the server treats it the same way as if there was no id.
type
type:
string
required
error
msg
type:
object
required

show 4 properties
code
type:
integer
Error code identifying the type of error:

1: Unable to process message - General processing error
2: Params required - Missing params object in command
3: Channels required - Missing channels array in subscribe
4: Subscription IDs required - Missing sids in unsubscribe
5: Unknown command - Invalid command name
6: Already subscribed - Duplicate subscription attempt
7: Unknown subscription ID - Subscription ID not found
8: Unknown channel name - Invalid channel in subscribe
9: Authentication required - Channel requires authenticated connection
10: Channel error - Channel-specific error
11: Invalid parameter - Malformed parameter value
12: Exactly one subscription ID is required - For update_subscription
13: Unsupported action - Invalid action for update_subscription
14: Market Ticker required - Missing market specification (market_ticker or market_id)
15: Action required - Missing action in update_subscription
16: Market not found - Invalid market_ticker or market_id
17: Internal error - Server-side processing error
18: Command timeout - Server timed out while processing command
19: shard_factor must be > 0 - Invalid shard_factor
20: shard_factor is required when shard_key is set - Missing shard_factor when shard_key is set
21: shard_key must be >= 0 and < shard_factor - Invalid shard_key
22: shard_factor must be <= 100 - shard_factor too large
msg
type:
string
Human-readable error message
market_id
type:
string
Market UUID if error is market-specific (optional)
market_ticker
type:
string
Market ticker if error is market-specific (optional)

Websockets
Connection Keep-Alive
WebSocket control frames for connection management.

Kalshi sends Ping frames (0x9) every 10 seconds with body heartbeat to maintain the connection. Clients should respond with Pong frames (0xA). Clients may also send Ping frames to which Kalshi will respond with Pong.
WSS

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Send
Ping
type:
string
Client sends Ping frame (0x9) to elicit Pong from Kalshi
Pong
type:
string
Client replies to Ping with Pong Frame (0xA)

Receive
Ping
type:
string
Kalshi sends Ping (0x9) with body 'heartbeat' to elicit Pong from client
Pong
type:
string
Kalshi responds to client Ping with Pong frame (0xA)
WebSocket Connection
Orderbook Updates

Websockets
Orderbook Updates
Real-time orderbook price level changes. Provides incremental updates to maintain a live orderbook.

Requirements:

Authentication required
Market specification required:
Use market_ticker (string) for a single market
Use market_tickers (array of strings) for multiple markets
market_id/market_ids are not supported for this channel
Sends orderbook_snapshot first, then incremental orderbook_delta updates
Use case: Building and maintaining a real-time orderbook
WSS
orderbook_delta

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Orderbook Snapshot
type:
object

show 4 properties
Complete view of the order book's aggregated price levels
Orderbook Delta
type:
object

show 4 properties
Update to be applied to the current order book view

Websockets
Market Ticker
Market price, volume, and open interest updates.

Requirements:

No additional channel-level authentication beyond the authenticated WebSocket connection
Market specification optional (omit to receive all markets)
Supports market_ticker/market_tickers and market_id/market_ids
Updates sent whenever any ticker field changes
Use case: Displaying current market prices and statistics
WSS
ticker

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Ticker Update
type:
object

show 3 properties
Market price ticker information
type
type:
string
required
ticker
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 14 properties
market_ticker
type:
string
Unique market identifier
market_id
type:
string
Unique market UUID
price_dollars
type:
string
Last traded price in dollars
yes_bid_dollars
type:
string
Best bid price for yes side in dollars
yes_ask_dollars
type:
string
Best ask price for yes side in dollars
volume_fp
type:
string
Fixed-point total contracts traded (2 decimals)
open_interest_fp
type:
string
Fixed-point open interest (2 decimals)
dollar_volume
type:
integer
Number of dollars traded in the market so far
dollar_open_interest
type:
integer
Number of dollars positioned in the market currently
yes_bid_size_fp
type:
string
Fixed-point contracts at best bid (2 decimals)
yes_ask_size_fp
type:
string
Fixed-point contracts at best ask (2 decimals)
last_trade_size_fp
type:
string
Fixed-point contracts in last trade (2 decimals)
ts
type:
integer
Unix timestamp for when the update happened (in seconds)
time
type:
string
Timestamp for when the update happened (RFC3339)
Orderbook Updates

Websockets
Public Trades
Public trade notifications when trades occur.

Requirements:

No additional channel-level authentication beyond the authenticated WebSocket connection
Market specification optional (omit to receive all trades)
Updates sent immediately after trade execution
Use case: Trade feed, volume analysis
WSS
trade

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Trade Update
type:
object

show 3 properties
Public trade information
type
type:
string
required
trade
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 7 properties
trade_id
type:
string
Unique identifier for the trade
market_ticker
type:
string
Unique market identifier
yes_price_dollars
type:
string
Yes side price in dollars
no_price_dollars
type:
string
No side price in dollars
count_fp
type:
string
Fixed-point contracts traded (2 decimals)
taker_side
type:
enum
Market side
Available options: yes, no
ts
type:
integer
Unix timestamp in seconds

Websockets
User Fills
Your order fill notifications. Requires authentication.

Requirements:

Authentication required
Market specification optional via market_ticker/market_tickers (omit to receive all your fills)
Supports update_subscription with add_markets / delete_markets
Updates sent immediately when your orders are filled
Use case: Tracking your trading activity
WSS
fill

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Fill Update
type:
object

show 3 properties
Private fill information for authenticated userWebsockets
Market Positions
Real-time updates of your positions in markets. Requires authentication.

Requirements:

Authentication required
Market specification optional (omit to receive all positions)
Filters are by market_ticker/market_tickers only; market_id/market_ids are not supported
Updates sent when your position changes due to trades, settlements, etc.
Monetary Values: All monetary values are returned as fixed-point dollar strings (_dollars suffix).

Use case: Portfolio tracking, position monitoring, P&L calculations
WSS
market_positions

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Market Position Update
type:
object

show 3 properties
Real-time position updates for authenticated user
User Fills

ebsockets
Market & Event Lifecycle
Market state changes and event creation notifications.

Requirements:

No additional channel-level authentication beyond the authenticated WebSocket connection
Receives all market and event lifecycle notifications (market_ticker filters are not supported)
Event creation notifications
Use case: Tracking market lifecycle including creation, de(activation), close date changes, determination, settlement, fractional trading updates, and price level structure changes
WSS
market_lifecycle_v2

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Market Lifecycle V2
type:
object

show 3 properties
Market lifecycle events (created, activated, deactivated, close_date_updated, determined, settled, fractional_trading_updated, price_level_structure_updated)
type
type:
string
required
market_lifecycle_v2
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 12 properties
event_type
type:
enum
Field to annotate which of the event type this event is for:

created - Market created
activated - Market activated
deactivated - Market deactivated
close_date_updated - Market close date updated
determined - Market determined
settled - Market settled
fractional_trading_updated - Market fractional trading setting changed
price_level_structure_updated - Market price level structure changed
Available options: created, deactivated, activated, close_date_updated, determined, settled, fractional_trading_updated, price_level_structure_updated
market_ticker
type:
string
Unique market identifier
open_ts
type:
integer
Optional - This key will ONLY exist when the market is created. Unix timestamp for when the market opened (in seconds)
close_ts
type:
integer
Optional - This key will ONLY exist when the market is created OR when the close date is updated. Unix timestamp for when the market is scheduled to close (in seconds). Will be updated in case of early determination markets
result
type:
string
Optional - This key will ONLY exist when the market is determined. Result of the market
determination_ts
type:
integer
Optional - This key will ONLY exist when the market is determined. Unix timestamp for when the market is determined (in seconds)
settlement_value
type:
string
Optional - This key will ONLY exist when the market is determined. Settlement value of the market in fixed-point dollars (e.g. "0.5000")
settled_ts
type:
integer
Optional - This key will ONLY exist when the market is settled. Unix timestamp for when the market is settled (in seconds)
is_deactivated
type:
boolean
Optional - This key will ONLY exist when the market is paused/unpaused. Boolean flag to indicate if trading is paused on an open market. This should only be interpreted for an open market
fractional_trading_enabled
type:
boolean
Optional - This key will exist when the market is created or when fractional trading is updated. Whether fractional trading is enabled for the market
price_level_structure
type:
enum
Optional - This key will exist when the market is created or when the price level structure is updated. The price level structure of the market
Available options: linear_cent, deci_cent, tapered_deci_cent
additional_metadata
type:
object

show 13 properties
Optional - This key will only be emitted when the market is created
name
type:
string
title
type:
string
yes_sub_title
type:
string
no_sub_title
type:
string
rules_primary
type:
string
rules_secondary
type:
string
can_close_early
type:
boolean
event_ticker
type:
string
expected_expiration_ts
type:
integer
strike_type
type:
string
floor_strike
type:
number
cap_strike
type:
number
custom_strike
type:
object
Event Lifecycle
type:
object

show 3 properties
Event creation notification
type
type:
string
required
event_lifecycle
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 7 properties
event_ticker
type:
string
Unique identifier for the event being created
title
type:
string
Title of event
subtitle
type:
string
Subtitle of event
collateral_return_type
type:
enum
Collateral return type, MECNET or DIRECNET of the event. Empty if there is no collateral return scheme for the event
Available options: MECNET, DIRECNET,
series_ticker
type:
string
Series ticker for the event
strike_date
type:
integer
Optional - Unix timestamp to indicate the strike date of the event if there is one
strike_period
type:
string
Optional - String to indicate the strike period of the event if there is one

ebsockets
Multivariate Market & Event Lifecycle
Multivariate event (MVE) market state changes and event creation notifications.

Requirements:

No additional channel-level authentication beyond the authenticated WebSocket connection
Receives all multivariate market lifecycle notifications (market_ticker filters are not supported)
Only emits lifecycle updates for multivariate events
Event creation notifications
Use case: Tracking multivariate market lifecycle including creation, de(activation), close date changes, determination, settlement
WSS
multivariate_market_lifecycle

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Multivariate Market Lifecycle
Multivariate market lifecycle events (created, activated, deactivated, close_date_updated, determined, settled)
Event Lifecycle
type:
object

show 3 properties
Event creation notification
type
type:
string
required
event_lifecycle
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 7 properties
event_ticker
type:
string
Unique identifier for the event being created
title
type:
string
Title of event
subtitle
type:
string
Subtitle of event
collateral_return_type
type:
enum
Collateral return type, MECNET or DIRECNET of the event. Empty if there is no collateral return scheme for the event
Available options: MECNET, DIRECNET,
series_ticker
type:
string
Series ticker for the event
strike_date
type:
integer
Optional - Unix timestamp to indicate the strike date of the event if there is one
strike_period
type:
string
Optional - String to indicate the strike period of the event if there is one
Market & Event Lifecycle

Websockets
Multivariate Lookups
Multivariate collection lookup notifications.

Requirements:

No additional channel-level authentication beyond the authenticated WebSocket connection
No filtering parameters; subscription is global
Use case: Tracking multivariate market relationships
WSS
multivariate

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Multivariate Lookup
type:
object

show 3 properties
Multivariate collection lookup notification
type
type:
string
required
multivariate_lookup
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 4 properties
collection_ticker
type:
string
event_ticker
type:
string
market_ticker
type:
string
selected_markets
type:
array

Websockets
Communications
Real-time Request for Quote (RFQ) and quote notifications. Requires authentication. **Requirements:** - Authentication required - Market specification ignored - Optional sharding for fanout control: - `shard_factor` (1-100) and `shard_key` (0 <= key < shard_factor) - RFQ events (RFQCreated, RFQDeleted) always sent - Quote events (QuoteCreated, QuoteAccepted, QuoteExecuted) are only sent if you created the quote OR you created the RFQ **Use case:** Tracking RFQs you create and quotes on your RFQs, or quotes you create on others' RFQs. Use QuoteExecuted to correlate fill messages with quotes via client_order_id.
WSS
communications

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
RFQ Created
type:
object

show 3 properties
Notification when an RFQ is created
type
type:
string
required
rfq_created
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 9 properties
id
type:
string
Unique identifier for the RFQ
creator_id
type:
string
Public communications ID of the RFQ creator (anonymized). Currently empty for rfq_created events.
market_ticker
type:
string
Market ticker for the RFQ
event_ticker
type:
string
Event ticker (optional)
contracts_fp
type:
string
Fixed-point contracts requested (2 decimals) (optional)
target_cost_dollars
type:
string
Target cost in dollars (optional)
created_ts
type:
string
Timestamp when the RFQ was created
mve_collection_ticker
type:
string
Multivariate event collection ticker (optional)
mve_selected_legs
type:
array
Selected legs for multivariate events (optional)
RFQ Deleted
type:
object

show 3 properties
Notification when an RFQ is deleted
type
type:
string
required
rfq_deleted
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 7 properties
id
type:
string
Unique identifier for the RFQ
creator_id
type:
string
Public communications ID of the RFQ creator (anonymized)
market_ticker
type:
string
Market ticker for the RFQ
event_ticker
type:
string
Event ticker (optional)
contracts_fp
type:
string
Fixed-point contracts requested (2 decimals) (optional)
target_cost_dollars
type:
string
Target cost in dollars (optional)
deleted_ts
type:
string
Timestamp when the RFQ was deleted
Quote Created
type:
object

show 3 properties
Notification when a quote is created on an RFQ
type
type:
string
required
quote_created
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 11 properties
quote_id
type:
string
Unique identifier for the quote
rfq_id
type:
string
Identifier of the RFQ this quote is for
quote_creator_id
type:
string
Public communications ID of the quote creator (anonymized)
market_ticker
type:
string
Market ticker for the quote
event_ticker
type:
string
Event ticker (optional)
yes_bid_dollars
type:
string
Yes side bid price in dollars
no_bid_dollars
type:
string
No side bid price in dollars
yes_contracts_offered_fp
type:
string
Fixed-point yes contracts offered (2 decimals) (optional)
no_contracts_offered_fp
type:
string
Fixed-point no contracts offered (2 decimals) (optional)
rfq_target_cost_dollars
type:
string
Target cost from the RFQ in dollars (optional)
created_ts
type:
string
Timestamp when the quote was created
Quote Accepted
type:
object

show 3 properties
Notification when a quote is accepted
type
type:
string
required
quote_accepted
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 12 properties
quote_id
type:
string
Unique identifier for the quote
rfq_id
type:
string
Identifier of the RFQ this quote is for
quote_creator_id
type:
string
Public communications ID of the quote creator (anonymized)
market_ticker
type:
string
Market ticker for the quote
event_ticker
type:
string
Event ticker (optional)
yes_bid_dollars
type:
string
Yes side bid price in dollars
no_bid_dollars
type:
string
No side bid price in dollars
accepted_side
type:
enum
Which side was accepted (yes/no) (optional)
Available options: yes, no
contracts_accepted_fp
type:
string
Fixed-point contracts accepted (2 decimals) (optional)
yes_contracts_offered_fp
type:
string
Fixed-point yes contracts offered (2 decimals) (optional)
no_contracts_offered_fp
type:
string
Fixed-point no contracts offered (2 decimals) (optional)
rfq_target_cost_dollars
type:
string
Target cost from the RFQ in dollars (optional)
Quote Executed
type:
object

show 33 properties
Notification when a quote is executed and orders are placed
const
type:
string
Unknown channel name
description
type:
string
Invalid channel in subscribe
const
type:
string
Authentication required
description
type:
string
Channel requires authenticated connection
const
type:
string
Channel error
description
type:
string
Channel-specific error
const
type:
string
Invalid parameter
description
type:
string
Malformed parameter value
const
type:
string
Exactly one subscription ID is required
description
type:
string
For update_subscription
const
type:
string
Unsupported action
description
type:
string
Invalid action for update_subscription
const
type:
string
Market Ticker required
description
type:
string
Missing market specification (market_ticker or market_id)
const
type:
string
Action required
description
type:
string
Missing action in update_subscription
const
type:
string
Market not found
description
type:
string
Invalid market_ticker or market_id
const
type:
string
Internal error
description
type:
string
Server-side processing error
const
type:
string
Command timeout
description
type:
string
Server timed out while processing command
const
type:
string
shard_factor must be > 0
description
type:
string
Invalid shard_factor
const
type:
string
shard_factor is required when shard_key is set
description
type:
string
Missing shard_factor when shard_key is set
const
type:
string
shard_key must be >= 0 and < shard_factor
description
type:
string
Invalid shard_key
const
type:
string
shard_factor must be <= 100
description
type:
string
shard_factor too large
type
type:
string
required
quote_executed
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 8 properties
quote_id
type:
string
Unique identifier for the quote that was executed
rfq_id
type:
string
Identifier of the RFQ this quote was for
quote_creator_id
type:
string
Anonymized identifier for the quote creator
rfq_creator_id
type:
string
Anonymized identifier for the RFQ creator
order_id
type:
string
Your order ID resulting from the quote execution. Use this to match with fill messages
client_order_id
type:
string
Your client order ID for the executed order. Use this to correlate with fill messages
market_ticker
type:
string
Market ticker for the executed quote
executed_ts
type:
string
Timestamp when the quote was executed and orders were placed

Websockets
Order Group Updates
Real-time order group lifecycle and limit updates. Requires authentication.

Requirements:

Authentication required
Market specification ignored
Updates sent when order groups are created, triggered, reset, deleted, or have limits updated
Use case: Tracking order group lifecycle and limits
WSS
order_group_updates

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
Order Group Updates
type:
object

show 4 properties
Order group lifecycle and limit updates for authenticated user
type
type:
string
required
order_group_updates
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
seq
type:
integer
required
Sequential number that should be checked if you want to guarantee you received all the messages. Used for snapshot/delta consistency
msg
type:
object
required

show 3 properties
event_type
type:
enum
Order group event type
Available options: created, triggered, reset, deleted, limit_updated
order_group_id
type:
string
Order group identifier
contracts_limit_fp
type:
string
Updated contracts limit in fixed-point (2 decimals). Present for "created" and "limit_updated" events only.

Websockets
User Orders
Real-time order created and updated notifications. Requires authentication.

Requirements:

Authentication required
Market specification optional via market_tickers (omit to receive all orders)
Supports update_subscription with add_markets / delete_markets actions
Updates sent when your orders are created, filled, canceled, or otherwise updated
Use case: Tracking your resting orders, fills, and cancellations in real time
WSS
user_orders

Security Schemes
apiKey
type:
apiKey
API key authentication required for WebSocket connections. The API key should be provided during the WebSocket handshake.

Receive
User Order Update
type:
object

show 3 properties
Real-time order updates for authenticated user
type
type:
string
required
user_order
sid
type:
integer
required
Server-generated subscription identifier (sid) used to identify the channel
msg
type:
object
required

show 21 properties
order_id
type:
string
Unique order identifier
user_id
type:
string
User identifier
ticker
type:
string
Unique market identifier
status
type:
enum
Current order status
Available options: resting, canceled, executed
side
type:
enum
Market side
Available options: yes, no
is_yes
type:
boolean
Whether the order is on the yes side. Equivalent to side == "yes"
yes_price_dollars
type:
string
Yes price in fixed-point dollars (4 decimals)
fill_count_fp
type:
string
Number of contracts filled in fixed-point (2 decimals)
remaining_count_fp
type:
string
Number of contracts remaining in fixed-point (2 decimals)
initial_count_fp
type:
string
Initial number of contracts in fixed-point (2 decimals)
taker_fill_cost_dollars
type:
string
Taker fill cost in fixed-point dollars (4 decimals)
maker_fill_cost_dollars
type:
string
Maker fill cost in fixed-point dollars (4 decimals)
taker_fees_dollars
type:
string
Taker fees in fixed-point dollars (4 decimals).
maker_fees_dollars
type:
string
Maker fees in fixed-point dollars (4 decimals).
client_order_id
type:
string
Client-provided order identifier
order_group_id
type:
string
Order group identifier, if applicable
self_trade_prevention_type
type:
enum
Self-trade prevention type
Available options: taker_at_cross, maker
created_time
type:
string
Order creation time in RFC 3339 format
last_update_time
type:
string
Last update time in RFC 3339 format
expiration_time
type:
string
Order expiration time in RFC 3339 format
subaccount_number
type:
integer
Subaccount number (0 for primary, 1-32 for subaccounts)

