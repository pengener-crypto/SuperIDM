package com.superidm.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Videocam
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.superidm.streaming.StreamInfo
import com.superidm.streaming.StreamQuality

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QualitySelectorSheet(
    streamInfo: StreamInfo,
    selectedQuality: StreamQuality?,
    onQualitySelected: (StreamQuality) -> Unit,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp)
        ) {
            Text(
                text = "Select Quality",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold
            )
            Text(
                text = streamInfo.title.ifEmpty { "Stream: ${streamInfo.type.name}" },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )

            Spacer(modifier = Modifier.height(16.dp))

            val videoQualities = streamInfo.qualities.filter { !it.isAudioOnly }
            val audioQualities = streamInfo.qualities.filter { it.isAudioOnly }

            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f, fill = false)
            ) {
                if (videoQualities.isNotEmpty()) {
                    item {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Default.Videocam, contentDescription = null)
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Video Qualities", style = MaterialTheme.typography.titleMedium)
                        }
                        Spacer(modifier = Modifier.height(8.dp))
                    }
                    items(videoQualities) { quality ->
                        QualityRow(
                            quality = quality,
                            isSelected = quality == selectedQuality,
                            onClick = { onQualitySelected(quality) }
                        )
                    }
                    item { Spacer(modifier = Modifier.height(16.dp)) }
                }

                if (audioQualities.isNotEmpty()) {
                    item {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Default.MusicNote, contentDescription = null)
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Audio Only", style = MaterialTheme.typography.titleMedium)
                        }
                        Spacer(modifier = Modifier.height(8.dp))
                    }
                    items(audioQualities) { quality ->
                        QualityRow(
                            quality = quality,
                            isSelected = quality == selectedQuality,
                            onClick = { onQualitySelected(quality) }
                        )
                    }
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

            Button(
                onClick = onConfirm,
                modifier = Modifier.fillMaxWidth(),
                enabled = selectedQuality != null
            ) {
                Text("Confirm Selection")
            }
            Spacer(modifier = Modifier.height(16.dp))
        }
    }
}

@Composable
fun QualityRow(
    quality: StreamQuality,
    isSelected: Boolean,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        RadioButton(
            selected = isSelected,
            onClick = onClick
        )
        Spacer(modifier = Modifier.width(8.dp))
        Column {
            Text(
                text = quality.label,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal
            )
            if (quality.bitrate > 0) {
                Text(
                    text = "${quality.bitrate / 1000} kbps ${if (quality.codecs.isNotEmpty()) "· ${quality.codecs}" else ""}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}
