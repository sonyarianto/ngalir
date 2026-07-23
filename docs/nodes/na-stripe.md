# na-stripe

Stripe API node: list, create, and manage payments and customers.

**Use cases:** stripe, payment, billing

## Inputs

```json
  action: string enum: [list_customers, create_customer, list_payments, create_payment, retrieve_payment] (required)
    Action to perform
  amount: integer
    Amount in cents (create_payment)
  currency: string default: usd
    Currency code (create_payment)
  customer_id: string
    Customer ID filter (list_payments)
  description: string
    Customer description (create_customer)
  email: string
    Customer email (create_customer)
  limit: integer default: 10
    Max results for list operations
  name: string
    Customer name (create_customer)
  payment_id: string
    Payment intent ID (retrieve_payment)
  source: string
    Payment method ID (create_payment)
```

## Outputs

```
  count: integer
  customer: object
  customers: array
  has_more: boolean
  id: string
  payment: object
  payments: array
  status: string
```

## Secrets

  - `NGALIR_SECRET_SECRET_KEY`

## Credentials

  - ID: `stripe_secret_key`
    Label: Stripe Secret Key
    Auth: api_key
    Field: secret_key (password, required)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  (none)
