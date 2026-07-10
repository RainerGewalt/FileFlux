// Package mqtt wraps the Paho MQTT client with the single outbound connection,
// auto-reconnect, credentials and Last-Will behaviour TrailTransfer needs.
package mqtt

import (
	"time"

	paho "github.com/eclipse/paho.mqtt.golang"
)

// Options configures the client.
type Options struct {
	Broker     string
	ClientID   string
	Username   string
	Password   string
	KeepAlive  time.Duration
	LWTTopic   string
	LWTPayload []byte
}

// Client is a thin wrapper exposing just the publish/subscribe surface the
// worker uses.
type Client struct {
	c         paho.Client
	onConnect func(*Client)
}

// New builds an (unconnected) client. Call OnConnect before Connect to register
// a handler that runs on every successful (re)connect.
func New(o Options) *Client {
	cl := &Client{}
	opts := paho.NewClientOptions().
		AddBroker(o.Broker).
		SetClientID(o.ClientID).
		SetKeepAlive(o.KeepAlive).
		SetCleanSession(true).
		SetAutoReconnect(true).
		SetConnectRetry(true).
		SetConnectRetryInterval(5 * time.Second).
		SetMaxReconnectInterval(60 * time.Second)

	if o.Username != "" {
		opts.SetUsername(o.Username)
		opts.SetPassword(o.Password)
	}
	if o.LWTTopic != "" {
		opts.SetBinaryWill(o.LWTTopic, o.LWTPayload, 1, true)
	}
	opts.SetOnConnectHandler(func(paho.Client) {
		if cl.onConnect != nil {
			cl.onConnect(cl)
		}
	})

	cl.c = paho.NewClient(opts)
	return cl
}

// OnConnect registers a handler invoked on each successful (re)connect, e.g. to
// (re)subscribe and republish retained health/capabilities.
func (c *Client) OnConnect(f func(*Client)) { c.onConnect = f }

// Connect blocks until the first connection succeeds or errors.
func (c *Client) Connect() error {
	tok := c.c.Connect()
	tok.Wait()
	return tok.Error()
}

// Publish sends a message and waits for the handoff to complete.
func (c *Client) Publish(topic string, qos byte, retain bool, payload []byte) error {
	tok := c.c.Publish(topic, qos, retain, payload)
	tok.Wait()
	return tok.Error()
}

// Subscribe registers a callback for a topic filter.
func (c *Client) Subscribe(topic string, qos byte, cb func(topic string, payload []byte)) error {
	tok := c.c.Subscribe(topic, qos, func(_ paho.Client, m paho.Message) {
		cb(m.Topic(), m.Payload())
	})
	tok.Wait()
	return tok.Error()
}

// Disconnect publishes nothing and closes the connection, waiting up to 250ms.
func (c *Client) Disconnect() { c.c.Disconnect(250) }
