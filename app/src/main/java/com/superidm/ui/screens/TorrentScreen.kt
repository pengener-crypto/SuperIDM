package com.superidm.ui.screens

import android.content.ClipboardManager
import android.content.Context
import android.os.Environment
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.viewmodel.TorrentViewModel
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TorrentScreen(
    viewModel: TorrentViewModel = hiltViewModel(),
    onBack: () -> Unit
) {
    val context = LocalContext.current
    val torrents by viewModel.torrents.collectAsState()
    val pendingFiles by viewModel.pendingFiles.collectAsState()
    val errorMessage by viewModel.error.collectAsState()
    
    var magnetInput by remember { mutableStateOf("") }
    val defaultSavePath = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "SuperIDM").absolutePath
    
    val filePickerLauncher = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        uri?.let { viewModel.loadTorrentFile(it, context) }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Torrents") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    titleContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(16.dp)
        ) {
            // Input Section
            OutlinedTextField(
                value = magnetInput,
                onValueChange = { magnetInput = it },
                label = { Text("Magnet URI") },
                modifier = Modifier.fillMaxWidth(),
                trailingIcon = {
                    IconButton(onClick = {
                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        magnetInput = clipboard.primaryClip?.getItemAt(0)?.text?.toString() ?: ""
                    }) {
                        Icon(Icons.Default.ContentPaste, contentDescription = "Paste")
                    }
                }
            )
            
            Spacer(modifier = Modifier.height(8.dp))
            
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Button(onClick = {
                    if (magnetInput.isNotBlank()) {
                        viewModel.addMagnet(magnetInput, defaultSavePath)
                        magnetInput = ""
                    }
                }) {
                    Icon(Icons.Default.AddLink, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(8.dp))
                    Text("Add Magnet")
                }

                FilledTonalButton(onClick = { filePickerLauncher.launch("application/x-bittorrent") }) {
                    Icon(Icons.Default.FileOpen, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(8.dp))
                    Text("Open .torrent")
                }
            }
            
            Spacer(modifier = Modifier.height(16.dp))
            
            // List Section
            if (torrents.isEmpty()) {
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(Icons.Default.Download, contentDescription = null, modifier = Modifier.size(64.dp), tint = MaterialTheme.colorScheme.primary)
                        Spacer(modifier = Modifier.height(16.dp))
                        Text("Add a torrent to get started", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            } else {
                LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(torrents.values.toList()) { torrent ->
                        ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(16.dp)) {
                                Text(torrent.name, fontWeight = FontWeight.Bold, maxLines = 1)
                                Spacer(modifier = Modifier.height(8.dp))
                                LinearProgressIndicator(
                                    progress = { torrent.progress / 100f },
                                    modifier = Modifier.fillMaxWidth()
                                )
                                Spacer(modifier = Modifier.height(4.dp))
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween
                                ) {
                                    Text("${String.format("%.1f", torrent.progress)}%", style = MaterialTheme.typography.bodySmall)
                                    Text(torrent.state, style = MaterialTheme.typography.bodySmall)
                                }
                                
                                Spacer(modifier = Modifier.height(8.dp))
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    AssistChip(
                                        onClick = { },
                                        label = { Text("↓ ${formatSize(torrent.downloadRate)}/s") },
                                        leadingIcon = { Icon(Icons.Default.ArrowDownward, contentDescription = null, modifier = Modifier.size(16.dp)) }
                                    )
                                    AssistChip(
                                        onClick = { },
                                        label = { Text("↑ ${formatSize(torrent.uploadRate)}/s") },
                                        leadingIcon = { Icon(Icons.Default.ArrowUpward, contentDescription = null, modifier = Modifier.size(16.dp)) }
                                    )
                                }
                                
                                Text("Seeds: ${torrent.seeders} | Peers: ${torrent.peers}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.End
                                ) {
                                    IconButton(onClick = {
                                        if (torrent.state == "paused") {
                                            viewModel.resume(torrent.infoHash)
                                        } else {
                                            viewModel.pause(torrent.infoHash)
                                        }
                                    }) {
                                        Icon(if (torrent.state == "paused") Icons.Default.PlayArrow else Icons.Default.Pause, contentDescription = "Play/Pause")
                                    }
                                    IconButton(onClick = { viewModel.remove(torrent.infoHash, false) }) {
                                        Icon(Icons.Default.Delete, contentDescription = "Delete", tint = MaterialTheme.colorScheme.error)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if (pendingFiles != null) {
            val selectedStates = remember(pendingFiles) { mutableStateListOf<Boolean>().apply { pendingFiles?.forEach { add(true) } } }
            
            AlertDialog(
                onDismissRequest = { viewModel.cancelFileSelection() },
                title = { Text("Select Files to Download") },
                text = {
                    LazyColumn {
                        itemsIndexed(pendingFiles!!) { index, file ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .toggleable(
                                        value = selectedStates[index],
                                        onValueChange = { selectedStates[index] = it }
                                    )
                                    .padding(vertical = 8.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Checkbox(
                                    checked = selectedStates[index],
                                    onCheckedChange = null
                                )
                                Spacer(modifier = Modifier.width(8.dp))
                                Column {
                                    Text(file.name, style = MaterialTheme.typography.bodyMedium, maxLines = 1)
                                    Text(formatSize(file.size), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                        }
                    }
                },
                confirmButton = {
                    TextButton(onClick = {
                        val selectedIndices = selectedStates.mapIndexedNotNull { index, isSelected -> if (isSelected) index else null }.toIntArray()
                        viewModel.confirmDownload(defaultSavePath, selectedIndices)
                    }) {
                        Text("Download")
                    }
                },
                dismissButton = {
                    TextButton(onClick = { viewModel.cancelFileSelection() }) {
                        Text("Cancel")
                    }
                }
            )
        }
        
        errorMessage?.let { error ->
            AlertDialog(
                onDismissRequest = { viewModel.dismissError() },
                title = { Text("Error") },
                text = { Text(error) },
                confirmButton = {
                    TextButton(onClick = { viewModel.dismissError() }) { Text("OK") }
                }
            )
        }
    }
}

private fun formatSize(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val exp = (Math.log(bytes.toDouble()) / Math.log(1024.0)).toInt()
    val pre = "KMGTPE"[exp - 1]
    return String.format("%.1f %sB", bytes / Math.pow(1024.0, exp.toDouble()), pre)
}
