-- Showcase 03: E-Commerce Order Fulfillment
-- Multi-agent pipeline for order processing, inventory, shipping, and notifications.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (handler, source, retry, cache, validate, output), agents, agent_bundle,
-- skills, flows (parallel, match, pipeline, fallback, on_error, run), lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
        ^net.write
    }
    ^db {
        ^db.read
        ^db.write
    }
    ^pay {
        ^pay.charge
        ^pay.refund
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema Order {
    id :: String
    customer_id :: String
    items :: List<String>
    total :: Float
    currency :: String
    shipping_address :: String
    status :: String
}

schema InventoryItem {
    sku :: String
    name :: String
    quantity :: Int
    warehouse :: String
    reorder_threshold :: Int
}

schema ShipmentLabel {
    tracking_number :: String
    carrier :: String
    estimated_delivery :: String
    weight_kg :: Float
    dimensions :: String
}

schema CustomerProfile {
    id :: String
    name :: String
    tier :: String
    lifetime_value :: Float
    preferred_carrier :: Optional<String>
}

-- ── Type Aliases ─────────────────────────────────────────────────

type OrderStatus = Received | Validated | Packed | Shipped | Delivered | Returned | Cancelled
type Carrier = DHL | FedEx | UPS | USPS | PostNord
type CustomerTier = Bronze | Silver | Gold | Platinum

-- ── Templates ────────────────────────────────────────────────────

template %fulfillment_summary {
    section ORDER
    ORDER_ID :: String                  <<order identifier>>
    VALIDATION :: String                <<payment and inventory check result>>
    section WAREHOUSE
    PICK_LIST :: String * 5             <<SKU | Name | Quantity | Bin Location>>
    PACK_INSTRUCTIONS :: String         <<special packing requirements>>
    section SHIPPING
    CARRIER :: String                   <<selected shipping carrier>>
    TRACKING :: String                  <<tracking number>>
    COST :: String                      <<shipping cost breakdown>>
    section CUSTOMER
    NOTIFICATION :: String              <<customer-facing status message>>
}

template %refund_report {
    section ASSESSMENT
    REASON :: String                    <<return reason category>>
    CONDITION :: String                 <<item condition assessment>>
    section RESOLUTION
    REFUND_AMOUNT :: String             <<calculated refund amount with breakdown>>
    RESTOCK :: String                   <<restock decision and warehouse routing>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %carrier_selection {
    <<SHIPPING RULES: Select carrier based on order value and customer tier.
    Platinum/Gold customers: always use {premium_carrier} with priority handling.
    Orders over {value_threshold}: use {premium_carrier}.
    International orders: use {international_carrier}.
    Default domestic: {default_carrier}. Always include tracking.
    For fragile items: add "FRAGILE" handling instructions and insurance.>>
    params {
        premium_carrier :: String = "FedEx Priority"
        default_carrier :: String = "USPS Ground"
        international_carrier :: String = "DHL Express"
        value_threshold :: String = "100.00"
    }
}

directive %customer_communication {
    <<TONE: Professional but warm. Address customer by first name.
    Include order number in every message. For delays, lead with empathy
    and provide a specific new estimated date — never say "soon".
    For {tier} tier customers, use premium language and mention their loyalty benefits.>>
    params {
        tier :: String = "standard"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #validate_order {
    description: <<Validate an incoming order: check payment authorization, verify inventory availability for all items, confirm shipping address deliverability, and apply any promotional discounts. Return a structured validation result with pass/fail for each check.>>
    requires: [^db.read, ^pay.charge]
    validate: strict
    retry: 2
    params {
        order_id :: String
        items :: String
        total :: Float
    }
    returns :: String
}

tool #check_inventory {
    description: <<Query real-time inventory levels for a list of SKUs across all warehouses. Return availability status, nearest warehouse with stock, and reorder alerts for items below threshold.>>
    requires: [^db.read]
    cache: "5m"
    retry: 3
    params {
        skus :: String
    }
    returns :: String
}

tool #create_shipment {
    description: <<Create a shipping label with the optimal carrier based on order details, customer tier, and destination. Apply carrier selection rules. Return tracking number, estimated delivery, and cost.>>
    requires: [^net.write]
    directives: [%carrier_selection]
    handler: "http POST https://api.shipping-hub.example.com/shipments"
    retry: 3
    params {
        order_id :: String
        address :: String
        weight :: String
        tier :: String
    }
    returns :: String
}

tool #notify_customer {
    description: <<Send a personalized order status notification to the customer via their preferred channel (email/SMS/push). Include order number, current status, and next expected action. For shipping notifications, include tracking link.>>
    requires: [^net.write]
    directives: [%customer_communication]
    handler: "http POST https://api.notifications.example.com/send"
    retry: 2
    params {
        customer_id :: String
        message :: String
        channel :: String
    }
    returns :: String
}

tool #process_return {
    description: <<Process a product return: assess item condition, calculate refund amount (full, partial, or store credit), update inventory, and initiate refund. Apply return policy rules based on purchase date and reason.>>
    requires: [^db.write, ^pay.refund]
    output: %refund_report
    validate: strict
    params {
        order_id :: String
        reason :: String
        condition :: String
    }
    returns :: String
}

tool #update_inventory {
    description: <<Update inventory counts after fulfillment or return. Trigger reorder alerts if stock falls below threshold.>>
    requires: [^db.write]
    source: ^db.update(table, record)
    params {
        table :: String
        record :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $express_fulfillment {
    description: <<Fast-track fulfillment for priority orders: validate, check inventory, and create shipment in an optimized sequence with minimal latency.>>
    tools: [#validate_order, #check_inventory, #create_shipment]
    strategy: <<parallel validation and inventory check, then shipment creation>>
    params {
        order_id :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @order_processor {
    permits: [^db.read, ^pay.charge, ^llm.query]
    tools: [#validate_order]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an order processing specialist. You validate orders with zero tolerance for errors — a missed validation check means lost revenue or a bad customer experience. Check every detail: payment, inventory, address, pricing. Report issues precisely.>>
}

agent @warehouse_agent {
    permits: [^db.read, ^db.write, ^llm.query]
    tools: [#check_inventory, #update_inventory]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a warehouse management system agent. You know the layout of every warehouse, the location of every bin, and the real-time stock levels. You optimize pick paths and flag reorder needs proactively. Accuracy is everything — a wrong count means a missed shipment.>>
    memory: [~warehouse_layout, ~reorder_history]
}

agent @shipping_agent {
    permits: [^net.write, ^llm.query]
    tools: [#create_shipment]
    skills: [$express_fulfillment]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a logistics specialist. You select the optimal carrier and service level for every shipment based on destination, weight, value, and customer tier. You negotiate the best rates and ensure every package has tracking. Speed matters, but reliability matters more.>>
}

agent @customer_service {
    permits: [^net.write, ^llm.query]
    tools: [#notify_customer]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a customer communications specialist. You craft notifications that are clear, warm, and actionable. You turn operational updates into positive customer experiences. You personalize messages based on customer tier and history.>>
}

agent @returns_agent {
    permits: [^db.write, ^pay.refund, ^llm.query]
    tools: [#process_return]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a returns processing specialist. You balance customer satisfaction with loss prevention. You assess return requests fairly, calculate accurate refunds, and route items for restocking or disposal. Every return is an opportunity to retain the customer.>>
}

agent_bundle @fulfillment_team {
    agents: [@order_processor, @warehouse_agent, @shipping_agent, @customer_service, @returns_agent]
    fallbacks: @shipping_agent ?> @warehouse_agent
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    postgres       "stdio npx @anthropic/mcp-server-postgres"
    stripe         "stdio npx @anthropic/mcp-server-stripe"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "oversold_inventory" {
    context: <<Race condition between two concurrent orders caused overselling of last-in-stock item>>
    rule: <<Always use optimistic locking with version checks when decrementing inventory — never trust cache for stock-critical operations>>
    severity: error
}

lesson "carrier_timeout" {
    context: <<FedEx API timeout during peak holiday season caused 200 orders to stall in fulfillment>>
    rule: <<Implement carrier failover — if primary carrier API times out after 10s, automatically fall back to secondary carrier>>
    severity: warning
}

lesson "refund_calculation" {
    context: <<Partial refund incorrectly included original shipping cost, overpaying by $8 average per return>>
    rule: <<Shipping costs should only be refunded when return reason is seller fault (wrong item, defective, not as described)>>
    severity: error
}

-- ── Flows ────────────────────────────────────────────────────────

-- Standard order fulfillment pipeline
flow fulfill_order(order_id :: String, items :: String, total :: Float, address :: String, customer_id :: String) -> String {
    -- Step 1: Validate and check inventory in parallel
    parallel {
        validation = @order_processor -> #validate_order(order_id, items, total)
        stock = @warehouse_agent -> #check_inventory(items)
    }

    -- Step 2: Create shipment
    shipment = @shipping_agent -> #create_shipment(order_id, address, "2.5", "standard")

    -- Step 3: Update inventory and notify customer in parallel
    parallel {
        updated = @warehouse_agent -> #update_inventory("inventory", items) on_error <<Inventory update deferred>>
        notified = @customer_service -> #notify_customer(customer_id, shipment, "email") on_error <<Notification queued for retry>>
    }

    return shipment
}

-- Return processing flow
flow process_order_return(order_id :: String, reason :: String) -> String {
    refund = @returns_agent -> #process_return(order_id, reason, "good")
    notified = @customer_service -> #notify_customer(order_id, refund, "email") on_error <<Notification skipped>>
    return refund
}

-- Tier-based fulfillment with match
flow tier_fulfillment(order_id :: String, items :: String, total :: Float, address :: String, tier :: String) -> String {
    validation = @order_processor -> #validate_order(order_id, items, total)

    shipment = match tier {
        "platinum" => @shipping_agent -> #create_shipment(order_id, address, "2.5", "platinum")
        "gold" => @shipping_agent -> #create_shipment(order_id, address, "2.5", "gold")
        _ => @shipping_agent -> #create_shipment(order_id, address, "2.5", "standard")
    }

    return shipment
}

-- Express pipeline: validate piped into shipment
flow express_ship(order_id :: String, items :: String, address :: String) -> String {
    result = @order_processor -> #validate_order(order_id, items, 0.0) |> @shipping_agent -> #create_shipment(order_id, address, "1.0", "platinum")
    return result
}

-- ── Tests ────────────────────────────────────────────────────────

test "order validation checks all fields" {
    result = @order_processor -> #validate_order("ORD-001", "SKU-A,SKU-B", 59.99)
    assert result
}

test "inventory check returns stock levels" {
    stock = @warehouse_agent -> #check_inventory("SKU-A,SKU-B,SKU-C")
    assert stock
}

test "return processing calculates refund" {
    refund = @returns_agent -> #process_return("ORD-001", "defective", "good")
    assert refund
}

test "full fulfillment pipeline completes" {
    result = run fulfill_order("ORD-TEST", "SKU-A", 29.99, "123 Main St", "CUST-001")
    assert result
}
