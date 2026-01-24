using System;
using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using OverlayHost.Models;

namespace OverlayHost.Services;

/// <summary>
/// Named Pipe IPC client for communicating with the Rust backend.
/// Handles auto-reconnect with exponential backoff.
/// </summary>
public class IpcService : IDisposable
{
    private const string PipeName = "meowcal-sub";
    private const int InitialReconnectDelayMs = 1000;
    private const int MaxReconnectDelayMs = 10000;
    private const int ConnectionTimeoutMs = 5000;
    private const double BackoffMultiplier = 2.0;

    private NamedPipeClientStream? _pipeClient;
    private StreamReader? _reader;
    private StreamWriter? _writer;
    private CancellationTokenSource? _cts;
    private Task? _receiveTask;
    private int _currentReconnectDelayMs = InitialReconnectDelayMs;

    /// <summary>
    /// Fired when a message is received from the backend.
    /// </summary>
    public event EventHandler<IpcMessage>? MessageReceived;

    /// <summary>
    /// Fired when the connection state changes.
    /// </summary>
    public event EventHandler<bool>? ConnectionStateChanged;

    /// <summary>
    /// Gets whether the client is currently connected.
    /// </summary>
    public bool IsConnected { get; private set; }

    /// <summary>
    /// Starts the IPC service and begins connection attempts.
    /// </summary>
    public Task StartAsync()
    {
        Debug.WriteLine("[IpcService] Starting IPC service...");
        _cts = new CancellationTokenSource();
        _receiveTask = ConnectLoopAsync(_cts.Token);
        return Task.CompletedTask;
    }

    /// <summary>
    /// Sends a message to the backend.
    /// </summary>
    public async Task SendMessageAsync(IpcMessage message)
    {
        if (!IsConnected || _writer == null)
        {
            Debug.WriteLine("[IpcService] Cannot send message: not connected");
            return;
        }

        try
        {
            var json = JsonSerializer.Serialize(message);
            await _writer.WriteLineAsync(json);
            await _writer.FlushAsync();
            Debug.WriteLine($"[IpcService] Sent message: {message.Type}");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[IpcService] Error sending message: {ex.Message}");
            HandleDisconnection();
        }
    }

    /// <summary>
    /// Helper to send a message with a typed payload.
    /// </summary>
    public Task SendAsync<T>(string type, T payload)
    {
        var message = new IpcMessage
        {
            Type = type,
            Payload = JsonSerializer.SerializeToElement(payload)
        };
        return SendMessageAsync(message);
    }

    /// <summary>
    /// Main connection loop with auto-reconnect.
    /// </summary>
    private async Task ConnectLoopAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            try
            {
                Debug.WriteLine("[IpcService] Attempting to connect to pipe...");
                await ConnectAsync(ct);

                // Reset reconnect delay on successful connection
                _currentReconnectDelayMs = InitialReconnectDelayMs;

                // Connection successful, start receiving
                Debug.WriteLine("[IpcService] Connected to backend ✅");
                SetConnectionState(true);

                await ReceiveMessagesAsync(ct);
            }
            catch (OperationCanceledException)
            {
                Debug.WriteLine("[IpcService] Connection loop cancelled");
                break;
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"[IpcService] Connection failed: {ex.Message}");
                HandleDisconnection();

                // Exponential backoff
                Debug.WriteLine($"[IpcService] Retrying in {_currentReconnectDelayMs}ms...");
                await Task.Delay(_currentReconnectDelayMs, ct);

                _currentReconnectDelayMs = Math.Min(
                    (int)(_currentReconnectDelayMs * BackoffMultiplier),
                    MaxReconnectDelayMs
                );
            }
        }
    }

    /// <summary>
    /// Connects to the named pipe.
    /// </summary>
    private async Task ConnectAsync(CancellationToken ct)
    {
        CleanupConnection();

        _pipeClient = new NamedPipeClientStream(
            ".",
            PipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous
        );

        using var timeoutCts = new CancellationTokenSource(ConnectionTimeoutMs);
        using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(ct, timeoutCts.Token);

        await _pipeClient.ConnectAsync(linkedCts.Token);

        _reader = new StreamReader(_pipeClient);
        _writer = new StreamWriter(_pipeClient) { AutoFlush = true };
    }

    /// <summary>
    /// Receives messages from the backend.
    /// </summary>
    private async Task ReceiveMessagesAsync(CancellationToken ct)
    {
        if (_reader == null)
        {
            throw new InvalidOperationException("Reader not initialized");
        }

        while (!ct.IsCancellationRequested)
        {
            try
            {
                var line = await _reader.ReadLineAsync();
                if (line == null)
                {
                    Debug.WriteLine("[IpcService] Connection closed by backend");
                    break;
                }

                var message = JsonSerializer.Deserialize<IpcMessage>(line);
                if (message != null)
                {
                    Debug.WriteLine($"[IpcService] Received message: {message.Type}");
                    MessageReceived?.Invoke(this, message);
                }
            }
            catch (IOException ex)
            {
                Debug.WriteLine($"[IpcService] Connection lost: {ex.Message}");
                break;
            }
            catch (JsonException ex)
            {
                Debug.WriteLine($"[IpcService] Failed to parse message: {ex.Message}");
                // Continue receiving despite parse errors
            }
        }

        HandleDisconnection();
    }

    /// <summary>
    /// Handles disconnection cleanup.
    /// </summary>
    private void HandleDisconnection()
    {
        if (!IsConnected)
        {
            return; // Already disconnected
        }

        Debug.WriteLine("[IpcService] Disconnected from backend");
        CleanupConnection();
        SetConnectionState(false);
    }

    /// <summary>
    /// Cleans up pipe resources.
    /// </summary>
    private void CleanupConnection()
    {
        _reader?.Dispose();
        _reader = null;

        _writer?.Dispose();
        _writer = null;

        _pipeClient?.Dispose();
        _pipeClient = null;
    }

    /// <summary>
    /// Sets connection state and fires event.
    /// </summary>
    private void SetConnectionState(bool isConnected)
    {
        if (IsConnected == isConnected)
        {
            return; // No change
        }

        IsConnected = isConnected;
        ConnectionStateChanged?.Invoke(this, isConnected);
    }

    /// <summary>
    /// Disposes the IPC service.
    /// </summary>
    public void Dispose()
    {
        Debug.WriteLine("[IpcService] Disposing...");

        _cts?.Cancel();
        _cts?.Dispose();

        CleanupConnection();

        _receiveTask?.Wait(1000); // Wait briefly for cleanup
        _receiveTask?.Dispose();

        GC.SuppressFinalize(this);
    }
}
